mod oft;

use ::oft::types::QuoteOFTResult;
use console_oft::{
    instructions::rate_limiter::{
        set_rate_limit_config::SetRateLimitConfigParams,
        set_rate_limit_global_config::SetRateLimitGlobalConfigParams,
    },
    state::RateLimitConfig,
};
use mollusk_svm_result::Check;

use crate::oft::helpers::*;

#[test]
fn quote_oft_returns_zero_max_amount_when_paused() {
    let fixture = fixture();

    fixture.context.process_and_validate_instruction(
        &set_default_paused_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.pauser_role_member,
            true,
        ),
        &[Check::success()],
    );
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

    let result = fixture.context.process_and_validate_instruction(
        &quote_oft_ix(
            &fixture.oft_store,
            &pause_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fee_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fixture.default_rate_limit,
            &rate_limit_pda(&fixture.oft_store, DST_EID as u128).0,
            quote_params(1_000, 0),
        ),
        &[Check::success()],
    );

    let quote: QuoteOFTResult = decode_return(&result.return_data);
    assert_eq!(quote.oft_limit.max_amount_ld, 0);
}

#[test]
fn quote_oft_uses_default_rate_limit_capacity_when_not_paused() {
    let fixture = fixture();

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

    let result = fixture.context.process_and_validate_instruction(
        &quote_oft_ix(
            &fixture.oft_store,
            &pause_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fee_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fixture.default_rate_limit,
            &rate_limit_pda(&fixture.oft_store, DST_EID as u128).0,
            quote_params(1_000, 0),
        ),
        &[Check::success()],
    );

    let quote: QuoteOFTResult = decode_return(&result.return_data);
    assert_eq!(quote.oft_limit.max_amount_ld, 1_000_000);
    assert!(quote.oft_fee_details.is_empty());
}

#[test]
fn quote_oft_allows_missing_per_eid_rate_limit_when_outflow_noops() {
    let fixture = fixture();

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
                    outbound_enabled: false,
                    inbound_enabled: true,
                    net_accounting_enabled: false,
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

    let result = fixture.context.process_and_validate_instruction(
        &quote_oft_ix(
            &fixture.oft_store,
            &pause_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fee_config_pda(&fixture.oft_store, DST_EID as u128).0,
            &fixture.default_rate_limit,
            &rate_limit_pda(&fixture.oft_store, DST_EID as u128).0,
            quote_params(1_000, 0),
        ),
        &[Check::success()],
    );

    let quote: QuoteOFTResult = decode_return(&result.return_data);
    assert_eq!(quote.oft_limit.max_amount_ld, u64::MAX);
}
