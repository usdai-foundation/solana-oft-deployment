use crate::helpers::{
    default_admin_key, expect_program_failure, insert_blacklist_entry,
    process_and_validate_transaction_instructions,
    setup_live_transfer_fixture_with_permanent_delegate, token_amount,
    transfer_checked_with_hook_ix, transfer_owner_key, LIVE_TRANSFER_AMOUNT,
    LIVE_TRANSFER_DECIMALS, LIVE_TRANSFER_STARTING_BALANCE,
};
use console_transfer_hook::{errors::HookError, AllowlistMode};
use mollusk_svm::result::Check;

#[test]
fn permanent_delegate_recovers_from_blocked_source() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        false,
        Some(permanent_delegate),
    );
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.source_token);

    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &permanent_delegate,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );

    assert_eq!(
        token_amount(&context, &fixture.source_token),
        LIVE_TRANSFER_STARTING_BALANCE - LIVE_TRANSFER_AMOUNT
    );
    assert_eq!(token_amount(&context, &fixture.destination_token), LIVE_TRANSFER_AMOUNT);
}

#[test]
fn permanent_delegate_recovers_from_blocked_source_even_when_paused() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        true,
        Some(permanent_delegate),
    );
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.source_token);

    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &permanent_delegate,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );
}

#[test]
fn permanent_delegate_cannot_recover_from_paused_only_source() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        true,
        Some(permanent_delegate),
    );

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &permanent_delegate,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::CannotRecoverFromAllowlisted);
}

#[test]
fn blocked_user_cannot_initiate_their_own_transfer() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        false,
        Some(permanent_delegate),
    );
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.source_token);

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
}

#[test]
fn permanent_delegate_cannot_transfer_from_non_blocked_source() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        false,
        Some(permanent_delegate),
    );

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &permanent_delegate,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::CannotRecoverFromAllowlisted);
}

#[test]
fn permanent_delegate_cannot_recover_in_open_mode() {
    let context = crate::helpers::context();
    let permanent_delegate = default_admin_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Open,
        false,
        Some(permanent_delegate),
    );

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &permanent_delegate,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::CannotRecoverFromAllowlisted);
}

#[test]
fn permanent_delegate_owner_transfer_checks_destination_blacklist() {
    let context = crate::helpers::context();
    let permanent_delegate = transfer_owner_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        false,
        Some(permanent_delegate),
    );
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.destination_token);

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &fixture.source_token,
        &fixture.mint,
        &fixture.destination_token,
        &permanent_delegate,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::DestinationBlocked);
}

#[test]
fn permanent_delegate_recovers_own_blocked_tokens() {
    let context = crate::helpers::context();
    let permanent_delegate = transfer_owner_key();
    let fixture = setup_live_transfer_fixture_with_permanent_delegate(
        &context,
        AllowlistMode::Blacklist,
        false,
        Some(permanent_delegate),
    );
    insert_blacklist_entry(&context, &fixture.hook_state, &fixture.source_token);

    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &fixture.source_token,
            &fixture.mint,
            &fixture.destination_token,
            &permanent_delegate,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );

    assert_eq!(
        token_amount(&context, &fixture.source_token),
        LIVE_TRANSFER_STARTING_BALANCE - LIVE_TRANSFER_AMOUNT
    );
    assert_eq!(token_amount(&context, &fixture.destination_token), LIVE_TRANSFER_AMOUNT);
}
