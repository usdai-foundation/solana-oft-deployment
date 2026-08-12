//! Initialize mint-scoped transfer-hook configuration.

use crate::{
    errors::HookError,
    state::{
        TransferHookState, ALLOWLIST_SEED, BYPASS_SEED, EXTRA_ACCOUNT_METAS_SEED,
        TRANSFER_HOOK_SEED,
    },
    AllowlistMode, Initialized, RoleMember, RoleType, ID,
};
use anchor_lang::{prelude::*, require_keys_eq, ToAccountInfo};
use anchor_spl::token_interface::Mint;
use rbac::init_default_admin;
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_token_2022::{
    extension::{transfer_hook::TransferHook, BaseStateWithExtensions, StateWithExtensions},
    state::Mint as MintState,
};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct InitTransferHookParams {
    /// Initial admin who will receive the DefaultAdmin role
    pub admin: Pubkey,
    /// Initial mint pause state
    pub paused: bool,
    /// Initial allowlist operating mode
    pub allowlist_mode: AllowlistMode,
}

#[init_default_admin(state = hook_state, role_type = RoleType, payer = payer, admin = params.admin)]
#[event_cpi]
#[derive(Accounts)]
#[instruction(params: InitTransferHookParams)]
pub struct InitTransferHook<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The authority configured on the mint's Token-2022 transfer-hook extension.
    /// Required to gate first-time creation of the ExtraAccountMetaList and
    /// TransferHookState PDAs for this mint.
    pub transfer_hook_authority: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Initialized via Anchor's `init`, then populated with ExtraAccountMetaList::init()
    #[account(
        init,
        payer = payer,
        space = ExtraAccountMetaList::size_of(EXTRA_ACCOUNT_COUNT).unwrap(),
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// Per-mint config. Mint-only seeds enforce one canonical config per mint.
    #[account(
        init,
        payer = payer,
        space = 8 + TransferHookState::INIT_SPACE,
        seeds = [TRANSFER_HOOK_SEED, mint.key().as_ref()],
        bump,
    )]
    pub hook_state: Account<'info, TransferHookState>,

    pub system_program: Program<'info, System>,
}

impl InitTransferHook<'_> {
    pub fn apply(ctx: &mut Context<Self>, params: &InitTransferHookParams) -> Result<()> {
        require_transfer_hook_authority(
            &ctx.accounts.mint,
            ctx.accounts.transfer_hook_authority.key(),
        )?;

        let account_metas = build_extra_account_metas(ctx.accounts.hook_state.key())?;

        ExtraAccountMetaList::init::<ExecuteInstruction>(
            &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
            &account_metas,
        )?;

        ctx.accounts.hook_state.set_inner(TransferHookState {
            mint: ctx.accounts.mint.key(),
            paused: params.paused,
            allowlist_mode: params.allowlist_mode,
            // Placeholders required to construct TransferHookState before DefaultAdmin setup.
            // setup_default_admin! runs immediately after set_inner and writes the
            // initial admin state while creating the first RoleMember PDA.
            current_default_admin: Pubkey::default(),
            pending_default_admin: Pubkey::default(),
            bump: ctx.bumps.hook_state,
        });

        // Only DefaultAdmin is bootstrapped here. Bypass operators must be granted
        // RoleType::BypassManager with the generated `grant_role` instruction.
        rbac::setup_default_admin!(ctx, hook_state, params.admin, RoleType, payer);

        emit_cpi!(Initialized {
            mint: ctx.accounts.mint.key(),
            transfer_hook_authority: ctx.accounts.transfer_hook_authority.key(),
            allowlist_mode: params.allowlist_mode,
            paused: params.paused,
        });

        Ok(())
    }
}

// =============================================================================
// ExtraAccountMeta Helpers
// =============================================================================

/// Token-2022 Transfer Hook account indices.
///
/// Indices 0-4 are the standard `Execute` accounts. Indices 5-9 are the
/// program-specific accounts registered in the ExtraAccountMetaList below.
/// Seed recipes use these positions directly through `Seed::AccountKey`.
///
/// | Index | Account                   | Source / derivation                      |
/// |-------|---------------------------|------------------------------------------|
/// | 0     | source token account      | standard Transfer Hook account           |
/// | 1     | mint                      | standard Transfer Hook account           |
/// | 2     | destination token account | standard Transfer Hook account           |
/// | 3     | authority                 | owner, delegate, or permanent delegate   |
/// | 4     | extra_account_meta_list   | standard Transfer Hook account           |
/// | 5     | hook_state                | ["transfer-hook", mint])                 |
/// | 6     | source_bypass_entry       | ["bypass", hook_state, source]           |
/// | 7     | source_allowlist_entry    | ["allowlist", hook_state, source]        |
/// | 8     | dest_allowlist_entry      | ["allowlist", hook_state, destination]   |
/// | 9     | authority_allowlist_entry | ["allowlist", hook_state, authority]     |
const SOURCE_ACCOUNT_INDEX: u8 = 0;
const DESTINATION_ACCOUNT_INDEX: u8 = 2;
const AUTHORITY_ACCOUNT_INDEX: u8 = 3;

const HOOK_STATE_INDEX: u8 = 5;

/// Number of extra accounts we register for the transfer hook.
/// These are appended after the standard 5 accounts (indices 5-9).
const EXTRA_ACCOUNT_COUNT: usize = 5;

/// Build a hook-state-scoped allowlist PDA using the account pubkey as the subject seed.
///
/// Uses `Seed::AccountKey` which takes the account's pubkey directly.
/// This is preferred over `Seed::AccountData` because:
/// - Works even when the account doesn't exist (first-time receivers)
/// - No need to read account data during resolution
///
/// Trade-off: Allowlist entries are per-ATA rather than per-wallet.
fn allowlist_from_account_key(
    seed: &[u8],
    account_index: u8,
) -> std::result::Result<ExtraAccountMeta, ProgramError> {
    ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal { bytes: seed.to_vec() },
            Seed::AccountKey { index: HOOK_STATE_INDEX },
            Seed::AccountKey { index: account_index },
        ],
        false, // is_signer
        false, // is_writable
    )
}

/// Build the array of ExtraAccountMeta for Token-2022 transfer hook resolution.
///
/// These define how Token-2022 resolves the extra accounts needed by our hook.
/// Order must match the field order in `TransferHook` Accounts struct.
fn build_extra_account_metas(
    hook_state: Pubkey,
) -> std::result::Result<[ExtraAccountMeta; EXTRA_ACCOUNT_COUNT], ProgramError> {
    Ok([
        // Hook state account
        ExtraAccountMeta::new_with_pubkey(&hook_state, false, false)?,
        // Bypass account
        ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal { bytes: BYPASS_SEED.to_vec() },
                Seed::AccountKey { index: HOOK_STATE_INDEX },
                Seed::AccountKey { index: SOURCE_ACCOUNT_INDEX },
            ],
            false,
            false,
        )?,
        // Allowlist accounts
        allowlist_from_account_key(ALLOWLIST_SEED, SOURCE_ACCOUNT_INDEX)?,
        allowlist_from_account_key(ALLOWLIST_SEED, DESTINATION_ACCOUNT_INDEX)?,
        allowlist_from_account_key(ALLOWLIST_SEED, AUTHORITY_ACCOUNT_INDEX)?,
    ])
}

/// Requires the signer that can mutate the Token-2022 transfer-hook extension.
fn require_transfer_hook_authority(
    token_mint: &InterfaceAccount<Mint>,
    transfer_hook_authority: Pubkey,
) -> Result<()> {
    let token_mint_info = token_mint.to_account_info();
    let token_mint_data = token_mint_info.try_borrow_data()?;
    let mint_state = StateWithExtensions::<MintState>::unpack(&token_mint_data)
        .map_err(|_| error!(HookError::InvalidMint))?;
    let transfer_hook = mint_state
        .get_extension::<TransferHook>()
        .map_err(|_| error!(HookError::MissingTransferHookExtension))?;

    require_keys_eq!(transfer_hook.program_id.0, ID, HookError::InvalidTransferHookProgram);
    require_keys_eq!(
        transfer_hook.authority.0,
        transfer_hook_authority,
        HookError::InvalidTransferHookAuthority
    );

    Ok(())
}
