use crate::helpers::{
    bypass_account_key, bypass_entry_pda, bypass_manager_key, context, init_default_fixture,
    insert_role_member, insert_wallet, read_account, role_member_pda, set_bypass_ix,
};
use console_transfer_hook::{state::BypassEntry, AllowlistMode, RoleType};
use mollusk_svm::result::Check;

#[test]
fn set_bypass_toggles_mint_scoped_entry() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let authority = bypass_manager_key();
    let account = bypass_account_key();
    let role_member =
        insert_role_member(&context, &hook_state, RoleType::BypassManager, &authority);
    let (bypass_entry, _) = bypass_entry_pda(&hook_state, &account);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    context.process_and_validate_instruction(
        &set_bypass_ix(
            &payer,
            &authority,
            &hook_state,
            &bypass_entry,
            &role_member,
            &account,
            true,
        ),
        &[
            Check::success(),
            Check::account(&bypass_entry).owner(&crate::helpers::program_id()).build(),
        ],
    );

    let entry: BypassEntry = read_account(&context, &bypass_entry);
    assert!(entry.initialized);

    context.process_and_validate_instruction(
        &set_bypass_ix(
            &payer,
            &authority,
            &hook_state,
            &bypass_entry,
            &role_member,
            &account,
            false,
        ),
        &[Check::success(), Check::account(&bypass_entry).closed().build()],
    );
}

#[test]
fn set_bypass_rejects_missing_bypass_manager_membership() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let authority = bypass_manager_key();
    let account = bypass_account_key();
    let (role_member, _) = role_member_pda(&hook_state, RoleType::BypassManager, &authority);
    let (bypass_entry, _) = bypass_entry_pda(&hook_state, &account);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    let result = context.process_instruction(&set_bypass_ix(
        &payer,
        &authority,
        &hook_state,
        &bypass_entry,
        &role_member,
        &account,
        true,
    ));
    assert!(result.program_result.is_err());
}
