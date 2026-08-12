use crate::{
    codec, inflow, serialize_into_owned_unchecked_account, split_and_validate_hook_accounts,
    OAppPeer, OFTError, OFTStore, RateLimit, RateLimitAddressExemption, DEFAULT_RATE_LIMIT_ID,
};
use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use oapp::{
    endpoint::{
        types::{ClearParams, SendComposeParams},
        ID as ENDPOINT_ID,
    },
    endpoint_cpi::{self, CLEAR_MIN_ACCOUNTS_LEN},
    types::LzReceiveParams,
    utils::try_load_account,
    OAppState,
};
use oft::{events::OFTReceived, types::OftType};
use spl_token_2022::{self, onchain::invoke_transfer_checked};

#[event_cpi]
#[derive(Accounts)]
#[instruction(params: LzReceiveParams)]
pub struct LzReceive<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    pub oft_store: Account<'info, OFTStore>,

    #[account(
        seeds = OAppPeer::seeds(&oft_store.key(), params.src_eid),
        bump = peer.bump,
        constraint = peer.address == params.sender @OFTError::InvalidSender
    )]
    pub peer: Box<Account<'info, OAppPeer>>,

    /// Default rate limiter PDA (id=0), used for fallback config and global state.
    #[account(
        mut,
        seeds = RateLimit::seeds(&oft_store.key(), DEFAULT_RATE_LIMIT_ID),
        bump
    )]
    pub default_rate_limit: Account<'info, RateLimit>,

    /// CHECK: Per-EID rate limiter PDA. Seeds validated by Anchor; deserialized via
    /// try_load_account and manually persisted by the handler after mutation.
    #[account(
        mut,
        seeds = RateLimit::seeds(&oft_store.key(), params.src_eid),
        bump
    )]
    pub rate_limit: UncheckedAccount<'info>,

    /// CHECK: Address exemption PDA for the recipient. Seeds validated by Anchor; existence
    /// checked via data_is_empty(). Not Option<Account<T>> — the client must always pass the
    /// correct PDA so the program (not the client) determines whether the exemption exists.
    #[account(
        seeds = RateLimitAddressExemption::seeds(&oft_store.key(), &to_address.key()),
        bump
    )]
    pub address_exemption: UncheckedAccount<'info>,

    // Boxed to reduce stack frame size (BPF limit: 4096 bytes)
    #[account(
        mut,
        seeds = OFTStore::token_escrow_seeds(&oft_store.key()),
        bump,
        token::authority = oft_store,
        token::mint = token_mint
    )]
    pub token_escrow: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: the wallet address to receive the token
    #[account(address = Pubkey::from(codec::msg::send_to(&params.message)?) @OFTError::InvalidTokenDest)]
    pub to_address: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = token_mint,
        associated_token::authority = to_address,
        associated_token::token_program = token_program
    )]
    pub token_dest: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        address = oft_store.token_mint,
        mint::token_program = token_program
    )]
    pub token_mint: Box<InterfaceAccount<'info, Mint>>,

    // Only used for native mint, the mint authority can be:
    //      1. a spl-token multisig account with oft_store as one of the signers, and the quorum
    //         **MUST** be 1-of-n. (recommended)
    //      2. or the mint_authority is oft_store itself.
    #[account(constraint = token_mint.mint_authority == COption::Some(mint_authority.key()) @OFTError::InvalidMintAuthority)]
    pub mint_authority: Option<UncheckedAccount<'info>>,

    pub token_program: Interface<'info, TokenInterface>,

    pub associated_token_program: Program<'info, AssociatedToken>,

    pub system_program: Program<'info, System>,
}

impl<'info> LzReceive<'info> {
    pub fn apply(
        ctx: &mut Context<'info, LzReceive<'info>>,
        params: &LzReceiveParams,
    ) -> Result<()> {
        // 1. Partition remaining_accounts into three contiguous slices:
        //
        //        [ clear | hook | compose ]
        //
        //    - clear:   accounts for the endpoint `clear()` CPI (CLEAR_MIN_ACCOUNTS_LEN).
        //    - hook:    optional token-2022 transfer-hook accounts.
        //    - compose: optional accounts for the endpoint `send_compose()` CPI.
        let (clear_accounts, rest) = ctx
            .remaining_accounts
            .split_at_checked(CLEAR_MIN_ACCOUNTS_LEN)
            .ok_or(OFTError::InsufficientRemainingAccounts)?;
        let (hook_accounts, compose_accounts) =
            split_and_validate_hook_accounts(&ctx.accounts.token_mint, rest)?;

        // 2. Clear the payload
        Self::clear_payload(&ctx.accounts.oft_store, params, clear_accounts)?;

        // 3. Convert amount (sd → ld)
        let amount_ld = ctx.accounts.oft_store.sd2ld(codec::msg::amount_sd(&params.message)?)?;

        // 4. Credit: _inflow + mint/unlock + transfer to dest (EVM alignment)
        let amount_received_ld = Self::credit(ctx, amount_ld, hook_accounts)?;

        // 5. Send compose message if present
        Self::send_compose_if_needed(
            &ctx.accounts.oft_store,
            ctx.accounts.to_address.key(),
            params,
            amount_received_ld,
            compose_accounts,
        )?;

        // 7. Emit event
        emit_cpi!(OFTReceived {
            oft_store: ctx.accounts.oft_store.key(),
            guid: params.guid,
            src_eid: params.src_eid,
            to: ctx.accounts.to_address.key(),
            amount_received_ld,
        });
        Ok(())
    }

    /// Clear the payload via LayerZero endpoint.
    fn clear_payload(
        oft_store: &Account<OFTStore>,
        params: &LzReceiveParams,
        clear_accounts: &[AccountInfo<'info>],
    ) -> Result<()> {
        endpoint_cpi::clear(
            ENDPOINT_ID,
            oft_store.key(),
            clear_accounts,
            &oft_store.signer_seeds(),
            ClearParams {
                receiver: oft_store.key(),
                src_eid: params.src_eid,
                sender: params.sender,
                nonce: params.nonce,
                guid: params.guid,
                message: params.message.clone(),
            },
        )?;
        Ok(())
    }

    /// EVM alignment: `_credit` in OFTBurnMint / OFTLockUnlock
    ///
    /// # Warning
    /// This implementation assumes lossless token transfers. It does **not** account for:
    ///   - Token2022 transfer fees > 0: a non-zero on-transfer fee means the amount that the
    ///     recipient actually receives (escrow → dest) is less than `amount_ld`, causing the
    ///     delivered amount to silently diverge from the cross-chain claim.
    ///   - Rebalancing / rebasing tokens: any supply adjustment between mint and delivery will
    ///     corrupt the credited amount in the same way.
    ///
    /// Integrators whose mint has either of these properties must override this function
    /// with custom credit logic that reconciles the actual delivered amount.
    fn credit(
        ctx: &mut Context<'info, LzReceive<'info>>,
        amount_ld: u64,
        hook_accounts: &[AccountInfo<'info>],
    ) -> Result<u64> {
        // 1. _inflow: apply rate limit (with address exemption)
        let is_address_exempt = !ctx.accounts.address_exemption.data_is_empty();
        let mut rate_limit: Option<RateLimit> = try_load_account(&ctx.accounts.rate_limit)?;
        inflow(
            &ctx.accounts.oft_store,
            &mut ctx.accounts.default_rate_limit,
            rate_limit.as_mut(),
            amount_ld,
            is_address_exempt,
        )?;
        serialize_into_owned_unchecked_account(&ctx.accounts.rate_limit, rate_limit.as_ref())?;

        // 2. Mint or unlock + deliver to recipient
        Self::mint_or_unlock(ctx, amount_ld, hook_accounts)?;

        Ok(amount_ld)
    }

    /// Deliver funds to the recipient — pair of `lock_or_burn` on the send side.
    /// For `BurnMint` the escrow is empty, so mint `amount_ld` first; for `LockUnlock`
    /// the escrow already holds the locked balance. Either way, transfer escrow → dest
    /// (which may trigger the transfer hook).
    ///
    /// `hook_accounts` layout: see [`split_and_validate_hook_accounts`].
    fn mint_or_unlock(
        ctx: &mut Context<'info, LzReceive<'info>>,
        amount_ld: u64,
        hook_accounts: &[AccountInfo<'info>],
    ) -> Result<()> {
        let seeds = &ctx.accounts.oft_store.signer_seeds();
        if ctx.accounts.oft_store.oft_type == OftType::BurnMint {
            let Some(mint_authority) = &ctx.accounts.mint_authority else {
                return Err(OFTError::InvalidMintAuthority.into());
            };
            let ix = spl_token_2022::instruction::mint_to(
                ctx.accounts.token_program.key,
                &ctx.accounts.token_mint.key(),
                &ctx.accounts.token_escrow.key(),
                mint_authority.key,
                &[&ctx.accounts.oft_store.key()],
                amount_ld,
            )?;
            anchor_lang::solana_program::program::invoke_signed(
                &ix,
                &[
                    ctx.accounts.token_escrow.to_account_info(),
                    ctx.accounts.token_mint.to_account_info(),
                    mint_authority.to_account_info(),
                    ctx.accounts.oft_store.to_account_info(),
                ],
                &[seeds],
            )?;
        }

        invoke_transfer_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.token_escrow.to_account_info(),
            ctx.accounts.token_mint.to_account_info(),
            ctx.accounts.token_dest.to_account_info(),
            ctx.accounts.oft_store.to_account_info(),
            hook_accounts,
            amount_ld,
            ctx.accounts.token_mint.decimals,
            &[seeds],
        )?;
        Ok(())
    }

    /// Send compose message if present in the original message.
    fn send_compose_if_needed(
        oft_store: &Account<OFTStore>,
        to_address: Pubkey,
        params: &LzReceiveParams,
        amount_received_ld: u64,
        compose_accounts: &[AccountInfo<'info>],
    ) -> Result<()> {
        if let Some(message) = codec::msg::compose_msg_with_sender(&params.message)? {
            endpoint_cpi::send_compose(
                ENDPOINT_ID,
                oft_store.key(),
                compose_accounts,
                &oft_store.signer_seeds(),
                SendComposeParams {
                    to: to_address,
                    guid: params.guid,
                    index: 0,
                    message: codec::compose_msg::encode(
                        params.nonce,
                        params.src_eid,
                        amount_received_ld,
                        message,
                    ),
                },
            )?;
        }
        Ok(())
    }
}
