use crate::{
    codec, combine_options,
    instructions::fee_config,
    msg_type, outflow, pause, serialize_into_owned_unchecked_account,
    split_and_validate_hook_accounts,
    state::{PauseConfig, RateLimitAddressExemption},
    EnforcedOptions, FeeConfig, OAppPeer, OFTError, OFTStore, RateLimit, DEFAULT_RATE_LIMIT_ID,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, TokenAccount, TokenInterface};
use oapp::{
    endpoint::{
        types::{MessagingReceipt, SendParams as EndpointSendParams},
        ID as ENDPOINT_ID,
    },
    endpoint_cpi,
    utils::try_load_account,
    OAppState as _,
};
use oft::{
    events::OFTSent,
    types::{OFTReceipt, OftType, SendParams},
};
use spl_token_2022::onchain::invoke_transfer_checked;

#[event_cpi]
#[derive(Accounts)]
#[instruction(params: SendParams)]
pub struct Send<'info> {
    pub signer: Signer<'info>,

    pub oft_store: Box<Account<'info, OFTStore>>,

    // Boxed to reduce stack frame size (BPF limit: 4096 bytes)
    #[account(
        seeds = OAppPeer::seeds(&oft_store.key(), params.dst_eid),
        bump = peer.bump
    )]
    pub peer: Box<Account<'info, OAppPeer>>,

    // Both enforced options PDAs (Send + SendAndCall) are included because SPL TLV
    // ExtraAccountMetaList resolves accounts from PDA seeds alone — it cannot inspect
    // variable-length instruction data to determine which msg_type applies.
    // The handler selects the correct PDA at runtime based on compose_msg presence.
    /// CHECK: Enforced options PDA for Send (msg_type=1). Seeds validated by Anchor;
    /// deserialized via try_load_account in handler.
    #[account(
        seeds = EnforcedOptions::seeds(
            &oft_store.key(),
            params.dst_eid,
            msg_type(params.compose_msg.as_ref()).into(),
        ),
        bump
    )]
    pub enforced_options: UncheckedAccount<'info>,

    /// CHECK: Per-EID pause config PDA. Seeds validated by Anchor; deserialized via
    /// try_load_account. Not Option<Account<T>> — the client must always pass the correct PDA
    /// so the program (not the client) determines whether the config is initialized.
    #[account(
        seeds = PauseConfig::seeds(&oft_store.key(), params.dst_eid),
        bump
    )]
    pub pause_config: UncheckedAccount<'info>,

    /// CHECK: Per-EID fee config PDA. Seeds validated by Anchor; deserialized via
    /// try_load_account. Not Option<Account<T>> — the client must always pass the correct PDA
    /// so the program (not the client) determines whether the config is initialized.
    #[account(
        seeds = FeeConfig::seeds(&oft_store.key(), params.dst_eid),
        bump
    )]
    pub fee_config: UncheckedAccount<'info>,

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
        seeds = RateLimit::seeds(&oft_store.key(), params.dst_eid),
        bump
    )]
    pub rate_limit: UncheckedAccount<'info>,

    /// CHECK: Address exemption PDA for the sender. Seeds validated by Anchor; existence checked
    /// via data_is_empty(). Not Option<Account<T>> — the client must always pass the correct PDA
    /// so the program (not the client) determines whether the exemption exists.
    #[account(
        seeds = RateLimitAddressExemption::seeds(&oft_store.key(), &signer.key()),
        bump
    )]
    pub address_exemption: UncheckedAccount<'info>,

    // Boxed to reduce stack frame size (BPF limit: 4096 bytes)
    #[account(
        mut,
        token::authority = signer,
        token::mint = token_mint
    )]
    pub token_source: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = OFTStore::token_escrow_seeds(&oft_store.key()),
        bump,
        token::authority = oft_store.key(),
        token::mint = token_mint
    )]
    pub token_escrow: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        address = oft_store.fee_deposit,
        token::mint = token_mint
    )]
    pub fee_deposit: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        address = oft_store.token_mint,
        mint::token_program = token_program
    )]
    pub token_mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> Send<'info> {
    /// EVM parity: mirrors `_send` in OFTCoreBaseUpgradeable.sol.
    /// pause → debit (_debitView + _outflow + _burnAndCollectFee) → _lzSend → emit
    pub fn apply(
        ctx: &mut Context<'info, Send<'info>>,
        params: &SendParams,
    ) -> Result<(MessagingReceipt, OFTReceipt)> {
        // 1. Pause check (EVM: whenNotPaused modifier)
        let pause_config = try_load_account(&ctx.accounts.pause_config)?;
        require!(
            !pause::is_paused(ctx.accounts.oft_store.default_paused, pause_config.as_ref()),
            OFTError::Paused
        );

        // 2. Debit: _debitView + _outflow + _burnAndCollectFee
        let (amount_sent_ld, amount_received_ld, _fee_ld, endpoint_offset) =
            Self::debit(ctx, params.amount_ld, params.min_amount_ld)?;

        // 3. _lzSend
        let msg_receipt = Self::send_lz_message(ctx, params, amount_received_ld, endpoint_offset)?;

        // 4. Emit event
        emit_cpi!(OFTSent {
            oft_store: ctx.accounts.oft_store.key(),
            guid: msg_receipt.guid,
            dst_eid: params.dst_eid,
            sender: ctx.accounts.signer.key(),
            from: ctx.accounts.token_source.key(),
            amount_sent_ld,
            amount_received_ld
        });

        Ok((msg_receipt, OFTReceipt { amount_sent_ld, amount_received_ld }))
    }

    /// EVM alignment: `_debit` in OFTBurnMintExtended / OFTLockUnlockExtended
    /// (`_debitView` → `_outflow` → `_burnAndCollectFee`).
    ///
    /// `_burnAndCollectFee` is split into:
    ///   - `lock_or_burn`: source → escrow (`amount_sent_ld`), then burn `amount_received_ld` if
    ///     `BurnMint` (no-op for `LockUnlock` — funds stay locked).
    ///   - `collect_fee`:  escrow → fee_deposit (`fee_ld`), if any.
    ///
    /// # Warning
    /// This implementation assumes lossless token transfers. It does **not** account for:
    ///   - Token2022 transfer fees > 0: a non-zero on-transfer fee means the amount that lands in
    ///     escrow is less than `amount_sent_ld`, desynchronising escrow balance accounting and the
    ///     cross-chain amount.
    ///   - Rebalancing / rebasing tokens: any supply adjustment between lock and burn will corrupt
    ///     the escrow ledger in the same way.
    ///
    /// Integrators whose mint has either of these properties must override this function
    /// with custom debit logic that reconciles the actual credited amount.
    ///
    /// `ctx.remaining_accounts` layout:
    ///   - With fee:    [transfer_hook..., fee_hook..., endpoint...]
    ///   - Without fee: [transfer_hook..., endpoint...]
    ///
    /// Returns `(amount_sent_ld, amount_received_ld, fee_ld, endpoint_offset)`,
    /// where `endpoint_offset` marks the start of endpoint accounts.
    fn debit(
        ctx: &mut Context<'info, Send<'info>>,
        amount_ld: u64,
        min_amount_ld: u64,
    ) -> Result<(u64, u64, u64, usize)> {
        // 1. Compute amounts (EVM: _debitView)
        let fee_config = try_load_account(&ctx.accounts.fee_config)?;
        let (amount_sent_ld, amount_received_ld) =
            debit_view(&ctx.accounts.oft_store, fee_config.as_ref(), amount_ld, min_amount_ld)?;
        let fee_ld = amount_sent_ld - amount_received_ld;

        // 2. Rate limit outflow (EVM: _outflow)
        let is_address_exempt = !ctx.accounts.address_exemption.data_is_empty();
        let mut rate_limit: Option<RateLimit> = try_load_account(&ctx.accounts.rate_limit)?;
        outflow(
            &ctx.accounts.oft_store,
            &mut ctx.accounts.default_rate_limit,
            rate_limit.as_mut(),
            amount_received_ld,
            is_address_exempt,
        )?;
        serialize_into_owned_unchecked_account(&ctx.accounts.rate_limit, rate_limit.as_ref())?;

        // 3. Lock or burn + collect fee (EVM: _burnAndCollectFee)
        let after_lock_offset = Self::lock_or_burn(ctx, amount_sent_ld, amount_received_ld)?;
        let endpoint_offset = Self::collect_fee(ctx, fee_ld, after_lock_offset)?;
        Ok((amount_sent_ld, amount_received_ld, fee_ld, endpoint_offset))
    }

    /// Move funds out of the sender (EVM: `safeTransferFrom(_from, address(this), amountSentLD)`).
    /// For `LockUnlock` the escrow holds the locked balance; for `BurnMint` the escrow is a
    /// transient hop — burn `amount_received_ld` so only the fee is left behind.
    ///
    /// Reads the leading hook prefix from `ctx.remaining_accounts` and returns the
    /// remaining-accounts offset just past it, where `collect_fee` resumes.
    fn lock_or_burn(
        ctx: &mut Context<'info, Send<'info>>,
        amount_sent_ld: u64,
        amount_received_ld: u64,
    ) -> Result<usize> {
        let (hook_accounts, _) =
            split_and_validate_hook_accounts(&ctx.accounts.token_mint, ctx.remaining_accounts)?;
        let hook_count = hook_accounts.len();

        // Transfer full amount from sender to escrow.
        invoke_transfer_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.token_source.to_account_info(),
            ctx.accounts.token_mint.to_account_info(),
            ctx.accounts.token_escrow.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            hook_accounts,
            amount_sent_ld,
            ctx.accounts.token_mint.decimals,
            &[],
        )?;

        // For BurnMint, burn the amount that will be received on the other side so only the fee
        // remains in escrow. For LockUnlock, keep the full amount in escrow.
        if ctx.accounts.oft_store.oft_type == OftType::BurnMint {
            token_interface::burn(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Burn {
                        mint: ctx.accounts.token_mint.to_account_info(),
                        from: ctx.accounts.token_escrow.to_account_info(),
                        authority: ctx.accounts.oft_store.to_account_info(),
                    },
                    &[&ctx.accounts.oft_store.signer_seeds()],
                ),
                amount_received_ld,
            )?;
        }

        Ok(hook_count)
    }

    /// EVM parity: `safeTransfer(feeDeposit(), amountSentLD - amountReceivedLD)`.
    /// Sweep the OFT fee left in escrow to `fee_deposit`. Takes the remaining-accounts
    /// `offset` past the lock hooks and returns the offset past the fee hooks too — the
    /// absolute start of the endpoint accounts. With no fee the offset is unchanged.
    fn collect_fee(
        ctx: &mut Context<'info, Send<'info>>,
        fee_ld: u64,
        offset: usize,
    ) -> Result<usize> {
        if fee_ld == 0 {
            return Ok(offset);
        }
        let after_lock = ctx
            .remaining_accounts
            .get(offset..)
            .ok_or(OFTError::InsufficientRemainingAccounts)?;
        let (fee_hook_accounts, _) =
            split_and_validate_hook_accounts(&ctx.accounts.token_mint, after_lock)?;
        let fee_hook_count = fee_hook_accounts.len();

        invoke_transfer_checked(
            ctx.accounts.token_program.key,
            ctx.accounts.token_escrow.to_account_info(),
            ctx.accounts.token_mint.to_account_info(),
            ctx.accounts.fee_deposit.to_account_info(),
            ctx.accounts.oft_store.to_account_info(),
            fee_hook_accounts,
            fee_ld,
            ctx.accounts.token_mint.decimals,
            &[&ctx.accounts.oft_store.signer_seeds()],
        )?;
        Ok(offset + fee_hook_count)
    }

    /// Send cross-chain message via LayerZero endpoint.
    ///
    /// # Arguments
    /// * `endpoint_accounts` - Accounts required for LayerZero endpoint send CPI
    fn send_lz_message(
        ctx: &Context<Send<'info>>,
        params: &SendParams,
        amount_received_ld: u64,
        endpoint_offset: usize,
    ) -> Result<MessagingReceipt> {
        let endpoint_accounts = ctx
            .remaining_accounts
            .get(endpoint_offset..)
            .ok_or(OFTError::InsufficientRemainingAccounts)?;

        let enforced_options = try_load_account::<EnforcedOptions>(&ctx.accounts.enforced_options)?;
        let amount_sd = ctx.accounts.oft_store.ld2sd(amount_received_ld);
        endpoint_cpi::send(
            ENDPOINT_ID,
            ctx.accounts.oft_store.key(),
            endpoint_accounts,
            &ctx.accounts.oft_store.signer_seeds(),
            EndpointSendParams {
                dst_eid: params.dst_eid,
                receiver: ctx.accounts.peer.get_address()?,
                message: codec::msg::encode(
                    params.to,
                    amount_sd,
                    ctx.accounts.signer.key(),
                    params.compose_msg.as_deref(),
                ),
                options: combine_options(enforced_options.as_ref(), &params.extra_options)?,
                native_fee: params.native_fee,
                lz_token_fee: params.lz_token_fee,
            },
        )
    }
}

/// EVM parity: `_debitView` in OFTCoreExtendedRBACUpgradeable.sol.
/// Returns `(amount_sent_ld, amount_received_ld)`. Fee is the difference.
pub fn debit_view(
    oft_store: &OFTStore,
    fee_config: Option<&FeeConfig>,
    amount_ld: u64,
    min_amount_ld: u64,
) -> Result<(u64, u64)> {
    let oft_fee = fee_config::get_oft_fee(oft_store.default_fee_bps, fee_config, amount_ld);
    let amount_received_ld = oft_store.remove_dust(amount_ld - oft_fee);

    require!(amount_received_ld >= min_amount_ld, OFTError::SlippageExceeded);
    Ok((amount_ld, amount_received_ld))
}
