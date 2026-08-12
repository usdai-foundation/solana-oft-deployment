use crate::OFTError;
use anchor_lang::{
    error,
    prelude::{AccountInfo, InterfaceAccount, Pubkey, Result},
    require_keys_eq, Key, ToAccountInfo,
};
use anchor_spl::{
    token_2022::spl_token_2022::{
        extension::{transfer_hook, StateWithExtensions},
        state::Mint as MintState,
    },
    token_interface::Mint,
};
use spl_discriminator::SplDiscriminate;
use spl_tlv_account_resolution::state::ExtraAccountMetaList;
use spl_transfer_hook_interface::{
    get_extra_account_metas_address, instruction::ExecuteInstruction,
};
use spl_type_length_value::state::TlvStateBorrowed;

/// Get the transfer hook program ID from mint extension.
///
/// Returns `None` when the mint doesn't have a transfer hook extension, or when the
/// extension exists but its program ID is unset.
pub fn get_transfer_hook_program_id(token_mint: &InterfaceAccount<Mint>) -> Result<Option<Pubkey>> {
    let token_mint_info = token_mint.to_account_info();
    let token_mint_data = token_mint_info.try_borrow_data()?;
    let token_mint_ext = StateWithExtensions::<MintState>::unpack(&token_mint_data)?;
    Ok(transfer_hook::get_program_id(&token_mint_ext))
}

/// Split `accounts` into `(hook_accounts, rest)` and validate the hook prefix.
///
/// The mint's `transfer_hook` extension is the source of truth for whether a
/// hook is set. When set, the caller is expected to lay out:
///
/// ```text
/// [0]      hook program (must match mint extension)
/// [1]      ExtraAccountMetaList PDA (optional — only when the hook publishes one)
/// [2..N+2] N resolved extra accounts
/// ```
///
/// Three resulting cases:
///
/// | mint hook | accounts[1] is meta_list PDA | prefix len | path                           |
/// |-----------|------------------------------|------------|--------------------------------|
/// | no        | n/a                          | 0          | no hook, forward everything    |
/// | yes       | yes                          | N + 2      | standard hook with N extras    |
/// | yes       | no (or only [0] present)     | 1          | hook with no extras            |
///
/// The expected meta_list PDA is derived from `(mint, hook_program)`; matching
/// by pubkey is sufficient because the PDA can only be written by the hook
/// program.
pub fn split_and_validate_hook_accounts<'a, 'info>(
    token_mint: &InterfaceAccount<'info, Mint>,
    accounts: &'a [AccountInfo<'info>],
) -> Result<(&'a [AccountInfo<'info>], &'a [AccountInfo<'info>])> {
    let Some(hook_id) = get_transfer_hook_program_id(token_mint)? else {
        return Ok((&[], accounts));
    };

    // Hook is set: hook_program must occupy [0].
    let hook_program = accounts.first().ok_or(OFTError::InsufficientRemainingAccounts)?;
    require_keys_eq!(hook_program.key(), hook_id, OFTError::InvalidTransferHookProgram);

    // [1] is meta_list if its key matches the canonical PDA; callers must only
    // pass this account when it is initialized (has data). Passing an uninitialized
    // PDA here is a caller error and will be rejected by TLV unpacking.
    // If [1] is absent or has a different key, the hook has no extras → split at 1.
    const HOOK_ONLY_LEN: usize = 1;
    const HOOK_WITH_META_LIST_BASE: usize = 2;
    let expected_meta_list = get_extra_account_metas_address(&token_mint.key(), &hook_id);
    let hook_accounts_len = match accounts.get(1) {
        Some(maybe_meta) if maybe_meta.key() == expected_meta_list => {
            let data = maybe_meta.try_borrow_data()?;
            let state = TlvStateBorrowed::unpack(&data)?;
            let extra_accounts_len =
                ExtraAccountMetaList::unpack_with_tlv_state::<ExecuteInstruction>(&state)?.len();
            HOOK_WITH_META_LIST_BASE + extra_accounts_len
        },
        _ => HOOK_ONLY_LEN,
    };

    accounts
        .split_at_checked(hook_accounts_len)
        .ok_or_else(|| OFTError::InsufficientRemainingAccounts.into())
}

/// Build the SPL Transfer Hook `Execute` instruction data that
/// `ExtraAccountMeta::resolve` expects: `[discriminator (8 bytes) | amount (8 bytes LE)]`.
///
/// This is the same payload Token-2022 hands to the hook's `Execute` entry point during
/// a real transfer; the resolver needs it because some extra-account seeds reference
/// fields inside the instruction data.
pub fn build_transfer_hook_execute_data(amount: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(16);
    data.extend_from_slice(ExecuteInstruction::SPL_DISCRIMINATOR_SLICE);
    data.extend_from_slice(&amount.to_le_bytes());
    data
}
