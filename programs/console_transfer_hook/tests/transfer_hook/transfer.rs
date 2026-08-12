use crate::helpers::{
    account_exists, expect_program_failure, expect_transaction_program_failure,
    insert_blacklist_entry, insert_role_member, insert_wallet, insert_whitelist_entry, pause_ix,
    process_and_validate_transaction_instructions, process_transaction_instructions, read_account,
    set_whitelisted_ix, setup_live_transfer_fixture, token_amount, transfer_checked_with_hook_ix,
    whitelister_key, LIVE_TRANSFER_AMOUNT, LIVE_TRANSFER_DECIMALS, LIVE_TRANSFER_STARTING_BALANCE,
};
use console_transfer_hook::{errors::HookError, state::TransferHookState, AllowlistMode, RoleType};
use mollusk_svm::result::Check;

#[test]
fn transfer_checked_with_hook_succeeds_when_open_and_unpaused() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, false);

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

    assert_eq!(
        token_amount(&context, &fixture.source_token),
        LIVE_TRANSFER_STARTING_BALANCE - LIVE_TRANSFER_AMOUNT
    );
    assert_eq!(token_amount(&context, &fixture.destination_token), LIVE_TRANSFER_AMOUNT);
}

#[test]
fn transfer_checked_with_hook_fails_when_paused() {
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
fn transfer_checked_with_hook_succeeds_in_blacklist_mode_without_entries() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Blacklist, false);

    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &fixture.source_owner,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );
}

#[test]
fn transfer_checked_with_hook_fails_for_blacklisted_source_destination_and_authority() {
    for (blocked, expected_error) in [
        ("source", HookError::SourceBlocked),
        ("destination", HookError::DestinationBlocked),
        ("authority", HookError::AuthorityBlocked),
    ] {
        let context = crate::helpers::context();
        let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Blacklist, false);
        let subject = match blocked {
            "source" => fixture.source_token,
            "destination" => fixture.destination_token,
            "authority" => fixture.source_owner,
            _ => unreachable!(),
        };
        insert_blacklist_entry(&context, &fixture.hook_state, &subject);

        let result = context.process_instruction(&transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &fixture.source_owner,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        ));
        expect_program_failure(&result, expected_error);
    }
}

#[test]
fn transfer_checked_with_hook_ignores_blacklist_entries_in_open_mode() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, false);
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.destination_token);

    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &fixture.source_owner,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );
}

#[test]
fn transfer_checked_with_hook_enforces_whitelist_for_source_destination_and_authority() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Whitelist, false);

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::SourceBlocked);

    insert_whitelist_entry(&context, &fixture.hook_state, &fixture.source_token);
    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::DestinationBlocked);

    insert_whitelist_entry(&context, &fixture.hook_state, &fixture.destination_token);
    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &fixture.source_owner,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::AuthorityBlocked);

    insert_whitelist_entry(&context, &fixture.hook_state, &fixture.source_owner);
    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &fixture.source_owner,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );
}

#[test]
fn transfer_checked_with_hook_sees_whitelist_updates_in_same_transaction() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Whitelist, false);
    let whitelister = whitelister_key();
    let role_member =
        insert_role_member(&context, &fixture.hook_state, RoleType::Whitelister, &whitelister);
    let (source_entry, _) =
        crate::helpers::allowlist_entry_pda(&fixture.hook_state, &fixture.source_token);
    let (destination_entry, _) =
        crate::helpers::allowlist_entry_pda(&fixture.hook_state, &fixture.destination_token);
    let (authority_entry, _) =
        crate::helpers::allowlist_entry_pda(&fixture.hook_state, &fixture.source_owner);

    insert_wallet(&context, whitelister);

    process_and_validate_transaction_instructions(
        &context,
        &[
            set_whitelisted_ix(
                &whitelister,
                &fixture.payer,
                &fixture.hook_state,
                &source_entry,
                &role_member,
                &fixture.source_token,
                true,
            ),
            set_whitelisted_ix(
                &whitelister,
                &fixture.payer,
                &fixture.hook_state,
                &destination_entry,
                &role_member,
                &fixture.destination_token,
                true,
            ),
            set_whitelisted_ix(
                &whitelister,
                &fixture.payer,
                &fixture.hook_state,
                &authority_entry,
                &role_member,
                &fixture.source_owner,
                true,
            ),
            transfer_checked_with_hook_ix(
                &context,
                &fixture.source_token,
                &fixture.mint,
                &fixture.destination_token,
                &fixture.source_owner,
                LIVE_TRANSFER_AMOUNT,
                LIVE_TRANSFER_DECIMALS,
            ),
        ],
        &[Check::success()],
    );

    assert!(account_exists(&context, &source_entry));
    assert!(account_exists(&context, &destination_entry));
    assert!(account_exists(&context, &authority_entry));
}

#[test]
fn transfer_checked_with_hook_rolls_back_same_transaction_pause_update_on_failure() {
    let context = crate::helpers::context();
    let fixture = setup_live_transfer_fixture(&context, AllowlistMode::Open, false);
    let pauser = crate::helpers::pauser_key();
    let pauser_role = insert_role_member(&context, &fixture.hook_state, RoleType::Pauser, &pauser);
    let source_before = token_amount(&context, &fixture.source_token);
    let destination_before = token_amount(&context, &fixture.destination_token);

    insert_wallet(&context, pauser);

    let result = process_transaction_instructions(
        &context,
        &[
            pause_ix(&pauser, &fixture.hook_state, &pauser_role),
            transfer_checked_with_hook_ix(
                &context,
                &fixture.source_token,
                &fixture.mint,
                &fixture.destination_token,
                &fixture.source_owner,
                LIVE_TRANSFER_AMOUNT,
                LIVE_TRANSFER_DECIMALS,
            ),
        ],
    );

    expect_transaction_program_failure(&result, 1, HookError::Paused);
    let hook_state: TransferHookState = read_account(&context, &fixture.hook_state);
    assert!(!hook_state.paused, "pause update should have rolled back");
    assert_eq!(token_amount(&context, &fixture.source_token), source_before);
    assert_eq!(token_amount(&context, &fixture.destination_token), destination_before);
}
