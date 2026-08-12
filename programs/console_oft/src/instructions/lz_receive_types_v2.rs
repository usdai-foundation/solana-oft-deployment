use crate::{
    build_transfer_hook_execute_data, codec, get_transfer_hook_program_id,
    seeds::EXTRA_ACCOUNT_METAS_SEED, OAppPeer, OFTError, OFTStore, RateLimit,
    RateLimitAddressExemption, DEFAULT_RATE_LIMIT_ID,
};
use anchor_lang::{prelude::*, solana_program};
use anchor_spl::{
    associated_token::get_associated_token_address_with_program_id,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use oapp::{
    common::{
        compact_accounts_with_alts, AccountMetaRef, AddressLocator, EXECUTION_CONTEXT_VERSION_1,
    },
    endpoint::ID as ENDPOINT_ID,
    lz_receive_types_v2::{
        get_accounts_for_clear, get_accounts_for_send_compose, Instruction, LzReceiveTypesV2Result,
    },
    types::LzReceiveParams,
    EVENT_SEED,
};
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;
use spl_type_length_value::state::TlvStateBorrowed;

/// Step 2 of the lz_receive_types_v2 flow:
/// Returns a full execution plan (ALTs + instruction account plan) for `lz_receive`.
///
/// Flow: build lz_receive accounts → append clear → append hook → append compose
#[derive(Accounts)]
#[instruction(params: LzReceiveParams)]
pub struct LzReceiveTypesV2<'info> {
    pub oft_store: Account<'info, OFTStore>,

    #[account(
        seeds = OFTStore::token_escrow_seeds(&oft_store.key()),
        bump,
        token::authority = oft_store,
        token::mint = token_mint,
    )]
    pub token_escrow: InterfaceAccount<'info, TokenAccount>,

    #[account(
        address = oft_store.token_mint,
        mint::token_program = token_program
    )]
    pub token_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Token dest ATA. Address enforced by Anchor constraint.
    #[account(
        address = get_associated_token_address_with_program_id(
            &Pubkey::from(codec::msg::send_to(&params.message)?),
            &oft_store.token_mint,
            &token_program.key(),
        ) @ OFTError::InvalidTokenDest
    )]
    pub token_dest: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,

    /// CHECK: transfer hook program account
    #[account(address = get_transfer_hook_program_id(&token_mint)?.unwrap() @ OFTError::InvalidTransferHookProgram)]
    pub transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: extra account meta list account
    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, token_mint.key().as_ref()],
        seeds::program = transfer_hook_program.as_ref().unwrap(),
        bump
    )]
    pub extra_account_meta_list: Option<UncheckedAccount<'info>>,
}

impl LzReceiveTypesV2<'_> {
    pub fn apply(
        ctx: &Context<LzReceiveTypesV2>,
        params: &LzReceiveParams,
    ) -> Result<LzReceiveTypesV2Result> {
        // 1. Derive PDAs and validate input accounts
        let derived = Self::derive_and_validate(ctx, params)?;

        // 2. Build lz_receive anchor accounts (mirrors lz_receive.rs struct order)
        let mut accounts = Self::build_lz_receive_accounts(ctx, &derived);

        // 3. Append remaining_accounts: clear → hook → compose
        Self::append_clear_accounts(&mut accounts, &ctx.accounts.oft_store, params);

        let amount_received_ld =
            ctx.accounts.oft_store.sd2ld(codec::msg::amount_sd(&params.message)?)?;
        Self::append_transfer_hook_accounts(&mut accounts, ctx, amount_received_ld)?;
        Self::append_compose_accounts(
            &mut accounts,
            &ctx.accounts.oft_store,
            params,
            &derived,
            amount_received_ld,
        )?;

        // 4. Compact with ALTs and return
        let alts = ctx.remaining_accounts;
        Ok(LzReceiveTypesV2Result {
            context_version: EXECUTION_CONTEXT_VERSION_1,
            alts: alts.iter().map(|a| a.key()).collect(),
            instructions: vec![Instruction::LzReceive {
                accounts: compact_accounts_with_alts(alts, accounts)?,
            }],
        })
    }

    // ── Step 1: Derive & validate ───────────────────────────────────────

    fn derive_and_validate(
        ctx: &Context<LzReceiveTypesV2>,
        params: &LzReceiveParams,
    ) -> Result<Derived> {
        let oft_store = &ctx.accounts.oft_store;
        let pid = ctx.program_id;

        // Decode recipient from message
        let to_address = Pubkey::from(codec::msg::send_to(&params.message)?);

        let token_program = ctx.accounts.token_program.key();

        // Derive all remaining PDAs
        let (peer, _) =
            Pubkey::find_program_address(OAppPeer::seeds(&oft_store.key(), params.src_eid), pid);
        let (default_rate_limit, _) = Pubkey::find_program_address(
            RateLimit::seeds(&oft_store.key(), DEFAULT_RATE_LIMIT_ID),
            pid,
        );
        let (rate_limit, _) =
            Pubkey::find_program_address(RateLimit::seeds(&oft_store.key(), params.src_eid), pid);
        let (address_exemption, _) = Pubkey::find_program_address(
            RateLimitAddressExemption::seeds(&oft_store.key(), &to_address),
            pid,
        );
        let (event_authority, _) = Pubkey::find_program_address(&[EVENT_SEED], pid);

        Ok(Derived {
            peer,
            default_rate_limit,
            rate_limit,
            address_exemption,
            to_address,
            token_program,
            mint_authority: ctx.accounts.token_mint.mint_authority.unwrap_or(pid.key()),
            event_authority,
        })
    }

    // ── Step 2: Build lz_receive account list ───────────────────────────

    /// Mirrors the field order of `LzReceive` accounts struct in lz_receive.rs.
    fn build_lz_receive_accounts(
        ctx: &Context<LzReceiveTypesV2>,
        d: &Derived,
    ) -> Vec<AccountMetaRef> {
        let oft_store = &ctx.accounts.oft_store;

        vec![
            // Signers
            writable(AddressLocator::Payer), //  0: payer
            // OFT state
            readonly(oft_store.key()), //  1: oft_store
            // LZ protocol
            readonly(d.peer), //  2: peer
            // Rate limit group
            writable(d.default_rate_limit), //  3: default_rate_limit
            writable(d.rate_limit),         //  4: rate_limit
            readonly(d.address_exemption),  //  5: address_exemption
            // Token accounts
            writable(ctx.accounts.token_escrow.key()), //  6: token_escrow
            readonly(d.to_address),                    //  7: to_address
            writable(ctx.accounts.token_dest.key()),   //  8: token_dest
            writable(ctx.accounts.token_mint.key()),   //  9: token_mint
            readonly(d.mint_authority),                // 10: mint_authority (or program_id)
            // Programs
            readonly(d.token_program),                  // 11: token_program
            readonly(anchor_spl::associated_token::ID), // 12: associated_token_program
            readonly(solana_program::system_program::ID), // 13: system_program
            // CPI event accounts (appended by #[event_cpi])
            readonly(d.event_authority),    // 14: event_authority
            readonly(ctx.program_id.key()), // 15: program_id
        ]
    }

    // ── Step 3a: Clear accounts ─────────────────────────────────────────

    fn append_clear_accounts(
        accounts: &mut Vec<AccountMetaRef>,
        oft_store: &Account<OFTStore>,
        params: &LzReceiveParams,
    ) {
        accounts.extend(get_accounts_for_clear(
            ENDPOINT_ID,
            &oft_store.key(),
            params.src_eid,
            &params.sender,
            params.nonce,
        ));
    }

    // ── Step 3b: Transfer hook accounts ─────────────────────────────────

    fn append_transfer_hook_accounts(
        accounts: &mut Vec<AccountMetaRef>,
        ctx: &Context<LzReceiveTypesV2>,
        amount_received_ld: u64,
    ) -> Result<()> {
        let Some(hook_program) = ctx.accounts.transfer_hook_program.as_ref() else {
            return Ok(());
        };
        accounts.push(readonly(hook_program.key()));

        // If the hook is set, the extra_account_meta_list must be present;
        let meta_list_account = ctx
            .accounts
            .extra_account_meta_list
            .as_ref()
            .ok_or(OFTError::InvalidExtraAccountMetaList)?;
        let meta_list_data = meta_list_account.try_borrow_data()?;
        if meta_list_data.is_empty() {
            return Ok(());
        }

        // Standard path: append meta_list + resolved extras.
        accounts.push(readonly(meta_list_account.key()));

        // Resolve extra accounts from ExtraAccountMetaList.
        // The resolver indexes accounts using the Token-2022 Execute layout:
        // [source, mint, destination, authority, extra_account_meta_list, ...resolved_extras].
        // Later extras may reference earlier resolved extra pubkeys via AccountKey seeds.
        // Their account data is not available in this account-discovery path, so AccountData
        // seeds are only supported for the fixed Execute accounts already present here.
        let state = TlvStateBorrowed::unpack(&meta_list_data)?;
        let extra_metas =
            ExtraAccountMetaList::unpack_with_tlv_state::<ExecuteInstruction>(&state)?;

        let escrow_info = ctx.accounts.token_escrow.to_account_info();
        let mint_info = ctx.accounts.token_mint.to_account_info();
        let dest_info = ctx.accounts.token_dest.to_account_info();
        let authority_info = ctx.accounts.oft_store.to_account_info();

        // Execute instruction account indices:
        // [0..=4] are the SPL Transfer Hook fixed accounts, including the
        // ExtraAccountMetaList account at index 4. They are supplied to this
        // console_oft receive-planning instruction, but their data is only useful
        // if the account already exists.
        //
        // The hook program account is part of lz_receive's hook account prefix,
        // but it is not part of this Execute seed index space.
        //
        // Each resolved extra is appended by pubkey before resolving the next
        // one, matching the index space used by SPL's CPI helper. Later extras
        // can therefore use Seed::AccountKey for prior extras. They cannot use
        // Seed::AccountData for newly resolved extras because this planner only
        // knows their pubkeys; Solana programs cannot read arbitrary account
        // data by pubkey alone.
        //
        // Transfer hooks should also avoid Seed::AccountData for fixed accounts
        // that lz_receive may create, such as token_dest. For a first-time
        // recipient ATA, receive planning knows the destination address before
        // lz_receive's init_if_needed creates the token account data.
        let mut account_key_data = vec![
            (*escrow_info.key, Some(escrow_info.try_borrow_data()?.to_vec())),
            (*mint_info.key, Some(mint_info.try_borrow_data()?.to_vec())),
            (*dest_info.key, Some(dest_info.try_borrow_data()?.to_vec())),
            (*authority_info.key, Some(authority_info.try_borrow_data()?.to_vec())),
            (meta_list_account.key(), Some(meta_list_data.to_vec())),
        ];

        let ix_data = build_transfer_hook_execute_data(amount_received_ld);
        for meta in extra_metas.iter() {
            let resolved = meta.resolve(&ix_data, hook_program.key, |i| {
                account_key_data.get(i).map(|(key, data)| (key, data.as_deref()))
            })?;
            accounts.push(AccountMetaRef {
                pubkey: resolved.pubkey.into(),
                is_writable: resolved.is_writable,
            });
            // We only need key-based progressive resolution. If future extras use
            // Seed::AccountData against earlier extras, load and store their data here.
            account_key_data.push((resolved.pubkey, None));
        }

        Ok(())
    }

    // ── Step 3c: Compose accounts ───────────────────────────────────────

    fn append_compose_accounts(
        accounts: &mut Vec<AccountMetaRef>,
        oft_store: &Account<OFTStore>,
        params: &LzReceiveParams,
        derived: &Derived,
        amount_received_ld: u64,
    ) -> Result<()> {
        if let Some(compose_message) = codec::msg::compose_msg_with_sender(&params.message)? {
            accounts.extend(get_accounts_for_send_compose(
                ENDPOINT_ID,
                &oft_store.key(),
                &derived.to_address,
                &params.guid,
                0,
                &codec::compose_msg::encode(
                    params.nonce,
                    params.src_eid,
                    amount_received_ld,
                    compose_message,
                ),
            ));
        }
        Ok(())
    }
}

// ─── Derived addresses ──────────────────────────────────────────────────────

/// All addresses needed to build the `lz_receive` account list that cannot be
/// read directly from the `LzReceiveTypesV2` struct (PDAs we derive, addresses
/// we decode from the message, optional accounts that may be replaced with
/// program-id placeholders).
struct Derived {
    // LZ protocol
    peer: Pubkey,
    // Rate limit group
    default_rate_limit: Pubkey,
    /// Per-EID rate-limit PDA derived from oft_store and src_eid.
    rate_limit: Pubkey,
    address_exemption: Pubkey,
    // Token / recipient
    to_address: Pubkey,
    token_program: Pubkey,
    /// Real mint authority, or program-id placeholder when COption::None.
    mint_authority: Pubkey,
    // CPI event
    event_authority: Pubkey,
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Shorthand: writable `AccountMetaRef`.
fn writable(pubkey: impl Into<AddressLocator>) -> AccountMetaRef {
    AccountMetaRef { pubkey: pubkey.into(), is_writable: true }
}

/// Shorthand: read-only `AccountMetaRef`.
fn readonly(pubkey: impl Into<AddressLocator>) -> AccountMetaRef {
    AccountMetaRef { pubkey: pubkey.into(), is_writable: false }
}
