use crate::helpers::{
    context, init_default_fixture, insert_role_member, insert_wallet, pause_ix, pauser_key,
    program_err, read_account, role_member_pda, unpause_ix, unpauser_key,
};
use console_transfer_hook::{errors::HookError, state::TransferHookState, AllowlistMode, RoleType};
use mollusk_svm::result::Check;

#[test]
fn pause_unpause_toggles_hook_state_pause_bit() {
    let context = context();
    let (_payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let pauser = pauser_key();
    let unpauser = unpauser_key();
    let pauser_role = insert_role_member(&context, &hook_state, RoleType::Pauser, &pauser);
    let unpauser_role = insert_role_member(&context, &hook_state, RoleType::Unpauser, &unpauser);

    insert_wallet(&context, pauser);
    insert_wallet(&context, unpauser);

    context.process_and_validate_instruction(
        &pause_ix(&pauser, &hook_state, &pauser_role),
        &[Check::success()],
    );
    context.process_and_validate_instruction(
        &pause_ix(&pauser, &hook_state, &pauser_role),
        &[Check::err(program_err(HookError::PauseStateIdempotent))],
    );
    let config: TransferHookState = read_account(&context, &hook_state);
    assert!(config.paused);

    context.process_and_validate_instruction(
        &unpause_ix(&unpauser, &hook_state, &unpauser_role),
        &[Check::success()],
    );
    context.process_and_validate_instruction(
        &unpause_ix(&unpauser, &hook_state, &unpauser_role),
        &[Check::err(program_err(HookError::PauseStateIdempotent))],
    );
    let config: TransferHookState = read_account(&context, &hook_state);
    assert!(!config.paused);
}

#[test]
fn pause_unpause_reject_missing_role_membership() {
    let context = context();
    let (_payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let pauser = pauser_key();
    let unpauser = unpauser_key();
    let (pauser_role, _) = role_member_pda(&hook_state, RoleType::Pauser, &pauser);
    let (unpauser_role, _) = role_member_pda(&hook_state, RoleType::Unpauser, &unpauser);

    insert_wallet(&context, pauser);
    insert_wallet(&context, unpauser);

    let result = context.process_instruction(&pause_ix(&pauser, &hook_state, &pauser_role));
    assert!(result.program_result.is_err());

    let pauser_role = insert_role_member(&context, &hook_state, RoleType::Pauser, &pauser);
    context.process_and_validate_instruction(
        &pause_ix(&pauser, &hook_state, &pauser_role),
        &[Check::success()],
    );

    let result = context.process_instruction(&unpause_ix(&unpauser, &hook_state, &unpauser_role));
    assert!(result.program_result.is_err());
}
