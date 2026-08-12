mod oft;

use anchor_lang::error::ErrorCode;
use console_oft::{
    errors::OFTError, events::RateLimitAddressExemptionUpdated,
    instructions::rate_limiter::set_rate_limit_address_exemption::SetRateLimitAddressExemptionParams,
    state::RoleType,
};
use mollusk_svm_result::Check;
use solana_pubkey::Pubkey;

use crate::oft::helpers::*;

#[test]
fn set_rate_limit_address_exemption_creates_account_and_view_returns_true() {
    let fixture = fixture();
    let exempt_user = Pubkey::new_unique();
    let (exemption, _bump) = rate_limit_address_exemption_pda(&fixture.oft_store, &exempt_user);

    // Inner instructions:
    //   [0] system program create_account (init exemption PDA)
    //   [1] __event_cpi (RateLimitAddressExemptionUpdated)
    let result = fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: true,
            },
        ),
        &[
            Check::success(),
            Check::inner_instruction_count(2),
            Check::account(&exemption).owner(&program_id()).rent_exempt().build(),
        ],
    );

    assert_event(
        &result.inner_instructions[1].instruction.data,
        RateLimitAddressExemptionUpdated {
            oft_store: to_program_pubkey(&fixture.oft_store),
            user: to_program_pubkey(&exempt_user),
            is_exempt: true,
        },
    );

    let result = fixture.context.process_and_validate_instruction(
        &is_rate_limit_address_exempt_ix(&fixture.oft_store, &exemption, &exempt_user),
        &[Check::success()],
    );
    assert!(decode_return::<bool>(&result.return_data));
}

#[test]
fn is_rate_limit_address_exempt_returns_false_for_missing_account() {
    let fixture = fixture();
    let user = Pubkey::new_unique();
    let (exemption, _) = rate_limit_address_exemption_pda(&fixture.oft_store, &user);

    let result = fixture.context.process_and_validate_instruction(
        &is_rate_limit_address_exempt_ix(&fixture.oft_store, &exemption, &user),
        &[Check::success()],
    );
    assert!(!decode_return::<bool>(&result.return_data));
}

#[test]
fn set_rate_limit_address_exemption_removes_account_and_view_returns_false() {
    let fixture = fixture();
    let exempt_user = Pubkey::new_unique();
    let (exemption, _) = rate_limit_address_exemption_pda(&fixture.oft_store, &exempt_user);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: true,
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: false,
            },
        ),
        &[Check::success(), Check::account(&exemption).closed().build()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &is_rate_limit_address_exempt_ix(&fixture.oft_store, &exemption, &exempt_user),
        &[Check::success()],
    );
    assert!(!decode_return::<bool>(&result.return_data));
}

#[test]
fn set_rate_limit_address_exemption_rejects_missing_manager_role() {
    let fixture = fixture();
    let exempt_user = Pubkey::new_unique();
    let (exemption, _) = rate_limit_address_exemption_pda(&fixture.oft_store, &exempt_user);
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::RateLimiterManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_rate_limit_address_exemption_ix(
        &fixture.delegate,
        &fixture.payer,
        &fixture.oft_store,
        &exemption,
        &missing_role_member,
        SetRateLimitAddressExemptionParams {
            user: to_program_pubkey(&exempt_user),
            is_exempt: true,
        },
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_rate_limit_address_exemption_rejects_idempotent_calls() {
    // EVM parity: `_setRateLimitAddressExemptions` reverts with
    // `ExemptionStateIdempotent` when the new state equals the current state.
    let fixture = fixture();
    let user = Pubkey::new_unique();
    let (exemption, _) = rate_limit_address_exemption_pda(&fixture.oft_store, &user);

    // set(false) on a user that's already non-exempt → should revert.
    let result = fixture.context.process_instruction(&set_rate_limit_address_exemption_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &exemption,
        &fixture.rate_limiter_role_member,
        SetRateLimitAddressExemptionParams { user: to_program_pubkey(&user), is_exempt: false },
    ));
    assert_eq!(result.program_result, program_failure(OFTError::ExemptionStateIdempotent));

    // Grant first.
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams { user: to_program_pubkey(&user), is_exempt: true },
        ),
        &[Check::success()],
    );

    // set(true) on an already-exempt user → should revert.
    let result = fixture.context.process_instruction(&set_rate_limit_address_exemption_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &exemption,
        &fixture.rate_limiter_role_member,
        SetRateLimitAddressExemptionParams { user: to_program_pubkey(&user), is_exempt: true },
    ));
    assert_eq!(result.program_result, program_failure(OFTError::ExemptionStateIdempotent));
}

#[test]
fn set_rate_limit_address_exemption_allows_recreation_after_removal() {
    let fixture = fixture();
    let exempt_user = Pubkey::new_unique();
    let (exemption, _bump) = rate_limit_address_exemption_pda(&fixture.oft_store, &exempt_user);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: true,
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: false,
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_address_exemption_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &exemption,
            &fixture.rate_limiter_role_member,
            SetRateLimitAddressExemptionParams {
                user: to_program_pubkey(&exempt_user),
                is_exempt: true,
            },
        ),
        &[Check::success()],
    );
}
