mod oft;

use ::oft::types::QuoteOFTResult;
use anchor_lang::error::ErrorCode;
use console_oft::{
    errors::OFTError,
    events::{DefaultFeeBpsSet, FeeBpsSet, FeeDepositSet},
    instructions::{
        fee_config::set_fee_bps::SetFeeBpsParams,
        rate_limiter::{
            set_rate_limit_config::SetRateLimitConfigParams,
            set_rate_limit_global_config::SetRateLimitGlobalConfigParams,
        },
    },
    state::{OFTStore, RateLimitConfig, RoleType},
};
use mollusk_svm_result::Check;
use solana_pubkey::Pubkey;

use crate::oft::helpers::*;

#[test]
fn set_default_fee_bps_rejects_missing_fee_config_manager_membership() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::FeeConfigManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_default_fee_bps_ix(
        &fixture.delegate,
        &fixture.oft_store,
        &missing_role_member,
        50,
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_default_fee_bps_rejects_invalid_bps() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_default_fee_bps_ix(
        &fixture.admin,
        &fixture.oft_store,
        &fixture.fee_manager_role_member,
        10_001,
    ));

    assert_eq!(result.program_result, program_failure(OFTError::InvalidFee));
}

#[test]
fn set_default_fee_bps_updates_oft_store() {
    let fixture = fixture();

    // Inner instructions: [0] __event_cpi (DefaultFeeBpsSet)
    let result = fixture.context.process_and_validate_instruction(
        &set_default_fee_bps_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.fee_manager_role_member,
            25,
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        DefaultFeeBpsSet { oft_store: to_program_pubkey(&fixture.oft_store), fee_bps: 25 },
    );

    let oft_store: OFTStore = read_account(&fixture.context, &fixture.oft_store);
    assert_eq!(oft_store.default_fee_bps, 25);
}

#[test]
fn set_fee_bps_rejects_missing_fee_config_manager_membership() {
    let fixture = fixture();
    let (fee_config, _) = fee_config_pda(&fixture.oft_store, DST_EID as u128);
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::FeeConfigManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_fee_bps_ix(
        &fixture.delegate,
        &fixture.payer,
        &fixture.oft_store,
        &fee_config,
        &missing_role_member,
        SetFeeBpsParams { id: DST_EID as u128, fee_bps: Some(10) },
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_fee_bps_rejects_invalid_bps() {
    let fixture = fixture();
    let (fee_config, _) = fee_config_pda(&fixture.oft_store, DST_EID as u128);

    let result = fixture.context.process_instruction(&set_fee_bps_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &fee_config,
        &fixture.fee_manager_role_member,
        SetFeeBpsParams { id: DST_EID as u128, fee_bps: Some(10_001) },
    ));

    assert_eq!(result.program_result, program_failure(OFTError::InvalidFee));
}

#[test]
fn set_fee_bps_creates_and_closes_per_eid_fee_config() {
    let fixture = fixture();
    let (fee_config, _bump) = fee_config_pda(&fixture.oft_store, DST_EID as u128);

    // Inner instructions:
    //   [0] system program create_account (init_if_needed fee_config PDA)
    //   [1] __event_cpi (FeeBpsSet)
    let result = fixture.context.process_and_validate_instruction(
        &set_fee_bps_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fee_config,
            &fixture.fee_manager_role_member,
            SetFeeBpsParams { id: DST_EID as u128, fee_bps: Some(50) },
        ),
        &[
            Check::success(),
            Check::inner_instruction_count(2),
            Check::account(&fee_config).owner(&program_id()).rent_exempt().build(),
        ],
    );

    assert_event(
        &result.inner_instructions[1].instruction.data,
        FeeBpsSet {
            oft_store: to_program_pubkey(&fixture.oft_store),
            id: DST_EID as u128,
            fee_bps: Some(50),
        },
    );

    let config = read_fee_config(&fixture.context, &fee_config);
    assert_eq!(config.fee_bps, 50);

    fixture.context.process_and_validate_instruction(
        &set_fee_bps_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fee_config,
            &fixture.fee_manager_role_member,
            SetFeeBpsParams { id: DST_EID as u128, fee_bps: None },
        ),
        &[Check::success(), Check::account(&fee_config).closed().build()],
    );
}

#[test]
fn set_fee_deposit_rejects_missing_default_admin_membership() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::DefaultAdmin, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_fee_deposit_ix(
        &fixture.delegate,
        &fixture.oft_store,
        &fixture.fee_deposit,
        &missing_role_member,
        fixture.fee_deposit,
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_fee_deposit_rejects_mismatched_pubkey() {
    let fixture = fixture();
    let random_fee_deposit = Pubkey::new_unique();
    insert_wallet(&fixture.context, random_fee_deposit);

    let result = fixture.context.process_instruction(&set_fee_deposit_ix(
        &fixture.admin,
        &fixture.oft_store,
        &fixture.fee_deposit,
        &fixture.default_admin_role_member,
        random_fee_deposit,
    ));

    assert_eq!(result.program_result, program_failure(OFTError::InvalidFeeDeposit));
}

#[test]
fn set_fee_deposit_updates_oft_store() {
    let fixture = fixture();
    let replacement_fee_deposit = Pubkey::new_unique();
    fixture.context.account_store.borrow_mut().insert(
        replacement_fee_deposit,
        token_account_account(&fixture.token_mint, &fixture.admin),
    );

    // Inner instructions: [0] __event_cpi (FeeDepositSet)
    let result = fixture.context.process_and_validate_instruction(
        &set_fee_deposit_ix(
            &fixture.admin,
            &fixture.oft_store,
            &replacement_fee_deposit,
            &fixture.default_admin_role_member,
            replacement_fee_deposit,
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        FeeDepositSet {
            oft_store: to_program_pubkey(&fixture.oft_store),
            fee_deposit: to_program_pubkey(&replacement_fee_deposit),
        },
    );

    let oft_store: OFTStore = read_account(&fixture.context, &fixture.oft_store);
    assert_eq!(oft_store.fee_deposit, to_program_pubkey(&replacement_fee_deposit));
}

#[test]
fn quote_oft_returns_expected_fee_breakdown() {
    let fixture = fixture();
    let (fee_config, _) = fee_config_pda(&fixture.oft_store, DST_EID as u128);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_global_config_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.rate_limiter_role_member,
            SetRateLimitGlobalConfigParams { use_global_state: true, is_globally_disabled: false },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams {
                id: 0,
                config: RateLimitConfig {
                    override_default_config: false,
                    outbound_enabled: true,
                    inbound_enabled: true,
                    net_accounting_enabled: true,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000_000,
                    inbound_limit: 2_000_000,
                    outbound_window: 60,
                    inbound_window: 120,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_fee_bps_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fee_config,
            &fixture.fee_manager_role_member,
            SetFeeBpsParams { id: DST_EID as u128, fee_bps: Some(50) },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &quote_oft_ix(
            &fixture.oft_store,
            &pause_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fee_config,
            &fixture.default_rate_limit,
            &rate_limit_pda(&fixture.oft_store, DST_EID as u128).0,
            quote_params(1_000, 0),
        ),
        &[Check::success()],
    );

    let quote: QuoteOFTResult = decode_return(&result.return_data);
    assert_eq!(quote.oft_receipt.amount_sent_ld, 1_000);
    assert_eq!(quote.oft_receipt.amount_received_ld, 995);
    assert_eq!(quote.oft_limit.max_amount_ld, 1_005_025);
    assert_eq!(quote.oft_fee_details.len(), 1);
    assert_eq!(quote.oft_fee_details[0].fee_amount_ld, 5);
    assert_eq!(quote.oft_fee_details[0].description, "Fee");
}
