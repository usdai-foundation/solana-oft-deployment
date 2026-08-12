use crate::helpers::{
    account_exists, bypass_entry_pda, bypass_manager_key, expect_program_failure,
    insert_blacklist_entry, insert_role_member, insert_wallet,
    process_and_validate_transaction_instructions, set_bypass_ix, setup_live_transfer_fixture,
    transfer_checked_with_hook_ix, user_key, LIVE_TRANSFER_AMOUNT, LIVE_TRANSFER_DECIMALS,
};
use console_transfer_hook::{errors::HookError, AllowlistMode, RoleType};
use mollusk_svm::result::Check;

fn init_source_bypass(
    context: &crate::helpers::TestContext,
    fixture: &crate::helpers::LiveTransferFixture,
) -> (solana_pubkey::Pubkey, solana_pubkey::Pubkey, solana_pubkey::Pubkey) {
    let authority = bypass_manager_key();
    let role_member =
        insert_role_member(context, &fixture.hook_state, RoleType::BypassManager, &authority);
    let (bypass_entry, _) = bypass_entry_pda(&fixture.hook_state, &fixture.source_token);

    insert_wallet(context, authority);

    context.process_and_validate_instruction(
        &set_bypass_ix(
            &fixture.payer,
            &authority,
            &fixture.hook_state,
            &bypass_entry,
            &role_member,
            &fixture.source_token,
            true,
        ),
        &[Check::success()],
    );

    (authority, role_member, bypass_entry)
}

#[test]
fn source_bypass_allows_transfer_while_paused() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, true);
    init_source_bypass(&context, &fixture);

    let ix = transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    );
    process_and_validate_transaction_instructions(&context, &[ix], &[Check::success()]);
}

#[test]
fn source_bypass_allows_transfer_to_blacklisted_destination() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Blacklist, false);
    init_source_bypass(&context, &fixture);
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.destination_token);

    let ix = transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    );
    process_and_validate_transaction_instructions(&context, &[ix], &[Check::success()]);
}

#[test]
fn source_bypass_allows_transfer_without_whitelist_entries() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Whitelist, false);
    init_source_bypass(&context, &fixture);

    let ix = transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    );
    process_and_validate_transaction_instructions(&context, &[ix], &[Check::success()]);
}

#[test]
fn unregistered_source_is_still_blocked_when_paused() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, true);

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::Paused);
}

#[test]
fn close_bypass_restores_compliance_checks() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, true);
    let (authority, role_member, bypass_entry) = init_source_bypass(&context, &fixture);

    context.process_and_validate_instruction(
        &set_bypass_ix(
            &fixture.payer,
            &authority,
            &fixture.hook_state,
            &bypass_entry,
            &role_member,
            &fixture.source_token,
            false,
        ),
        &[Check::success(), Check::account(&bypass_entry).closed().build()],
    );

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::Paused);
}

#[test]
fn close_bypass_rejects_missing_bypass_manager_membership() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, true);
    let (_authority, _role_member, bypass_entry) = init_source_bypass(&context, &fixture);
    let non_manager = user_key();
    let (missing_role_member, _) =
        crate::helpers::role_member_pda(&fixture.hook_state, RoleType::BypassManager, &non_manager);

    insert_wallet(&context, non_manager);

    let result = context.process_instruction(&set_bypass_ix(
        &fixture.payer,
        &non_manager,
        &fixture.hook_state,
        &bypass_entry,
        &missing_role_member,
        &fixture.source_token,
        false,
    ));
    assert!(result.program_result.is_err());
    assert!(account_exists(&context, &bypass_entry));
}
