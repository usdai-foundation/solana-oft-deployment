mod oft;

use anchor_lang::error::ErrorCode;
use console_oft::{
    errors::OFTError,
    events::{RateLimitConfigUpdated, RateLimitGlobalConfigUpdated, RateLimitStateUpdated},
    instructions::rate_limiter::{
        set_rate_limit_config::SetRateLimitConfigParams,
        set_rate_limit_global_config::SetRateLimitGlobalConfigParams,
        set_rate_limit_state::SetRateLimitStateParams,
    },
    state::{
        OFTStore, RateLimit, RateLimitConfig, RateLimitState, RateLimitUsages, RoleType,
        DEFAULT_RATE_LIMIT_ID,
    },
};
use mollusk_svm_result::Check;

use crate::oft::helpers::*;

fn sample_config(outbound_limit: u64, inbound_limit: u64) -> RateLimitConfig {
    RateLimitConfig {
        override_default_config: true,
        outbound_enabled: true,
        inbound_enabled: true,
        net_accounting_enabled: true,
        address_exemption_enabled: false,
        outbound_limit,
        inbound_limit,
        outbound_window: 60,
        inbound_window: 60,
    }
}

#[test]
fn set_rate_limit_global_config_rejects_missing_manager_role() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::RateLimiterManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_rate_limit_global_config_ix(
        &fixture.delegate,
        &fixture.oft_store,
        &missing_role_member,
        SetRateLimitGlobalConfigParams { use_global_state: true, is_globally_disabled: false },
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_rate_limit_global_config_updates_oft_store() {
    let fixture = fixture();

    // Inner instructions: [0] __event_cpi (RateLimitGlobalConfigUpdated)
    let result = fixture.context.process_and_validate_instruction(
        &set_rate_limit_global_config_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.rate_limiter_role_member,
            SetRateLimitGlobalConfigParams { use_global_state: true, is_globally_disabled: false },
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        RateLimitGlobalConfigUpdated {
            oft_store: to_program_pubkey(&fixture.oft_store),
            use_global_state: true,
            is_globally_disabled: false,
        },
    );

    let store: OFTStore = read_account(&fixture.context, &fixture.oft_store);
    assert!(store.use_global_state);
    assert!(!store.is_globally_disabled);
}

#[test]
fn set_rate_limit_config_sets_default_config() {
    let fixture = fixture();
    let config = RateLimitConfig {
        override_default_config: false,
        outbound_enabled: true,
        inbound_enabled: true,
        net_accounting_enabled: true,
        address_exemption_enabled: false,
        outbound_limit: 1_000_000,
        inbound_limit: 2_000_000,
        outbound_window: 60,
        inbound_window: 120,
    };

    // Inner instructions: [0] __event_cpi (RateLimitConfigUpdated)
    let result = fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id: 0, config: config.clone() },
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        RateLimitConfigUpdated {
            oft_store: to_program_pubkey(&fixture.oft_store),
            id: 0,
            config: config.clone(),
        },
    );

    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    let stored = &default_rl.config;
    assert!(stored.outbound_enabled);
    assert!(stored.inbound_enabled);
    assert!(stored.net_accounting_enabled);
    assert_eq!(stored.outbound_limit, 1_000_000);
    assert_eq!(stored.inbound_limit, 2_000_000);
    assert_eq!(stored.outbound_window, 60);
    assert_eq!(stored.inbound_window, 120);
}

#[test]
fn set_rate_limit_state_rejects_missing_manager_role() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::RateLimiterManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_rate_limit_state_ix(
        &fixture.delegate,
        &fixture.payer,
        &fixture.oft_store,
        &fixture.default_rate_limit,
        &missing_role_member,
        SetRateLimitStateParams {
            id: 0,
            state: RateLimitState { outbound_usage: 0, inbound_usage: 0, last_updated: 0 },
        },
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_rate_limit_state_updates_state() {
    let fixture = fixture();
    let current_time = current_unix_timestamp();
    let state =
        RateLimitState { outbound_usage: 10_000, inbound_usage: 8_000, last_updated: current_time };

    // Inner instructions: [0] __event_cpi (RateLimitStateUpdated)
    let result = fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams { id: 0, state: state.clone() },
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        RateLimitStateUpdated {
            oft_store: to_program_pubkey(&fixture.oft_store),
            id: 0,
            state: state.clone(),
        },
    );

    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    let stored = &default_rl.state;
    assert_eq!(stored.outbound_usage, 10_000);
    assert_eq!(stored.inbound_usage, 8_000);
    assert_eq!(stored.last_updated, current_time);
}

#[test]
fn set_rate_limit_state_rejects_future_timestamp() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_rate_limit_state_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &fixture.default_rate_limit,
        &fixture.rate_limiter_role_member,
        SetRateLimitStateParams {
            id: 0,
            state: RateLimitState { outbound_usage: 0, inbound_usage: 0, last_updated: u64::MAX },
        },
    ));

    assert_eq!(result.program_result, program_failure(OFTError::LastUpdatedInFuture));
}

#[test]
fn set_rate_limit_state_can_reset_usage_to_zero() {
    let fixture = fixture();
    let current_time = current_unix_timestamp();

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: 0,
                state: RateLimitState {
                    outbound_usage: 10_000,
                    inbound_usage: 8_000,
                    last_updated: current_time,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: 0,
                state: RateLimitState {
                    outbound_usage: 0,
                    inbound_usage: 0,
                    last_updated: current_time,
                },
            },
        ),
        &[Check::success()],
    );

    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    let state = &default_rl.state;
    assert_eq!(state.outbound_usage, 0);
    assert_eq!(state.inbound_usage, 0);
}

// ===================== SetRateLimitConfig: id > 0 branches =====================

#[test]
fn set_rate_limit_config_sets_per_eid_config() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit, _bump) = rate_limit_pda(&fixture.oft_store, id);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id, config: sample_config(1_000_000, 2_000_000) },
        ),
        &[Check::success(), Check::account(&rate_limit).owner(&program_id()).rent_exempt().build()],
    );

    let rl = read_rate_limit(&fixture.context, &rate_limit);
    assert!(rl.config.override_default_config);
    assert_eq!(rl.config.outbound_limit, 1_000_000);
    assert_eq!(rl.config.inbound_limit, 2_000_000);
}

#[test]
fn set_rate_limit_config_rejects_missing_rate_limiter_for_default_id() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_rate_limit_config_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &program_id(),
        &fixture.rate_limiter_role_member,
        SetRateLimitConfigParams { id: 0, config: sample_config(1_000, 1_000) },
    ));
    assert_eq!(result.program_result, program_failure(ErrorCode::ConstraintSeeds));
}

#[test]
fn set_rate_limit_config_rejects_missing_rate_limiter_for_per_eid() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_rate_limit_config_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &program_id(),
        &fixture.rate_limiter_role_member,
        SetRateLimitConfigParams { id: DST_EID as u128, config: sample_config(1_000, 1_000) },
    ));
    assert_eq!(result.program_result, program_failure(ErrorCode::ConstraintSeeds));
}

// ===================== SetRateLimitState: id > 0 branches =====================

#[test]
fn set_rate_limit_state_sets_per_eid_state() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit, _bump) = rate_limit_pda(&fixture.oft_store, id);
    let current_time = current_unix_timestamp();

    // init the PDA first via set_rate_limit_config
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id, config: sample_config(1_000, 1_000) },
        ),
        &[Check::success()],
    );

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id,
                state: RateLimitState {
                    outbound_usage: 123,
                    inbound_usage: 456,
                    last_updated: current_time,
                },
            },
        ),
        &[Check::success()],
    );

    let rl = read_rate_limit(&fixture.context, &rate_limit);
    assert_eq!(rl.state.outbound_usage, 123);
    assert_eq!(rl.state.inbound_usage, 456);
    assert_eq!(rl.state.last_updated, current_time);
}

#[test]
fn set_rate_limit_state_rejects_missing_rate_limiter_for_default_id() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_rate_limit_state_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &program_id(),
        &fixture.rate_limiter_role_member,
        SetRateLimitStateParams { id: 0, state: RateLimitState::default() },
    ));
    assert_eq!(result.program_result, program_failure(ErrorCode::ConstraintSeeds));
}

#[test]
fn set_rate_limit_state_rejects_missing_rate_limiter_for_per_eid() {
    let fixture = fixture();

    let result = fixture.context.process_instruction(&set_rate_limit_state_ix(
        &fixture.admin,
        &fixture.payer,
        &fixture.oft_store,
        &program_id(),
        &fixture.rate_limiter_role_member,
        SetRateLimitStateParams { id: DST_EID as u128, state: RateLimitState::default() },
    ));
    assert_eq!(result.program_result, program_failure(ErrorCode::ConstraintSeeds));
}

// ===================== CheckpointRateLimit =====================

#[test]
fn checkpoint_rate_limit_rejects_missing_manager_role() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::RateLimiterManager, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&checkpoint_rate_limit_ix(
        &fixture.delegate,
        &fixture.oft_store,
        &fixture.default_rate_limit,
        &fixture.default_rate_limit,
        &missing_role_member,
        DEFAULT_RATE_LIMIT_ID,
    ));
    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

// ===================== RateLimits::rate_limits =====================

#[test]
fn rate_limits_returns_default_for_id_zero() {
    let fixture = fixture();
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id: 0, config: sample_config(1_234, 5_678) },
        ),
        &[Check::success()],
    );

    // The rate_limiter UncheckedAccount must still satisfy seeds for id=0 (the default PDA).
    let result = fixture.context.process_and_validate_instruction(
        &rate_limits_ix(&fixture.oft_store, &fixture.default_rate_limit, 0),
        &[Check::success()],
    );

    let rl: Option<RateLimit> = decode_return(&result.return_data);
    let rl = rl.expect("id=0 must always return Some from default RateLimit PDA");
    assert_eq!(rl.config.outbound_limit, 1_234);
    assert_eq!(rl.config.inbound_limit, 5_678);
}

#[test]
fn rate_limits_returns_none_for_uninitialized_per_eid() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit_key, _) = rate_limit_pda(&fixture.oft_store, id);

    let result = fixture.context.process_and_validate_instruction(
        &rate_limits_ix(&fixture.oft_store, &rate_limit_key, id),
        &[Check::success()],
    );

    let rl: Option<RateLimit> = decode_return(&result.return_data);
    assert!(rl.is_none());
}

#[test]
fn rate_limits_returns_per_eid_when_initialized() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit_key, _) = rate_limit_pda(&fixture.oft_store, id);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit_key,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id, config: sample_config(42, 84) },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &rate_limits_ix(&fixture.oft_store, &rate_limit_key, id),
        &[Check::success()],
    );

    let rl: Option<RateLimit> = decode_return(&result.return_data);
    let rl = rl.expect("initialized per-EID should be Some");
    assert_eq!(rl.config.outbound_limit, 42);
    assert_eq!(rl.config.inbound_limit, 84);
}

// ===================== GetRateLimitUsages::get_rate_limit_usages =====================

#[test]
fn get_rate_limit_usages_reads_default_when_global() {
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
            SetRateLimitConfigParams { id: 0, config: sample_config(1_000, 2_000) },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &get_rate_limit_usages_ix(
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.default_rate_limit,
            0,
        ),
        &[Check::success()],
    );
    let usages: RateLimitUsages = decode_return(&result.return_data);
    assert_eq!(usages.outbound_available, 1_000);
    assert_eq!(usages.inbound_available, 2_000);
}

#[test]
fn get_rate_limit_usages_reads_default_for_id_zero_when_not_global() {
    // EVM parity: id=0 always reads the default RateLimit PDA, mirroring
    // `rateLimits[0]` in the EVM mapping, regardless of `use_global_state`.
    let fixture = fixture();
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id: 0, config: sample_config(3_000, 4_000) },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &get_rate_limit_usages_ix(
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.default_rate_limit,
            0,
        ),
        &[Check::success()],
    );
    let usages: RateLimitUsages = decode_return(&result.return_data);
    assert_eq!(usages.outbound_available, 3_000);
    assert_eq!(usages.inbound_available, 4_000);
}

#[test]
fn get_rate_limit_usages_returns_decayed_usage_when_globally_disabled() {
    const CLOCK: i64 = 1_700_000_000;
    const WINDOW: u64 = 60;
    let fixture = fixture_with_clock(CLOCK);
    let clock = CLOCK as u64;

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams {
                id: DEFAULT_RATE_LIMIT_ID,
                config: RateLimitConfig {
                    override_default_config: false,
                    outbound_enabled: true,
                    inbound_enabled: true,
                    net_accounting_enabled: true,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000,
                    inbound_limit: 1_000,
                    outbound_window: WINDOW,
                    inbound_window: WINDOW,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: DEFAULT_RATE_LIMIT_ID,
                state: RateLimitState {
                    outbound_usage: 800,
                    inbound_usage: 700,
                    last_updated: clock - WINDOW / 2,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_global_config_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.rate_limiter_role_member,
            SetRateLimitGlobalConfigParams { use_global_state: true, is_globally_disabled: true },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &get_rate_limit_usages_ix(
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.default_rate_limit,
            DEFAULT_RATE_LIMIT_ID,
        ),
        &[Check::success()],
    );
    let usages: RateLimitUsages = decode_return(&result.return_data);

    // elapsed = WINDOW/2, so each direction decays by 500. Global disable only
    // overrides the available capacities; it does not hide the remaining usage.
    assert_eq!(usages.outbound_usage, 300);
    assert_eq!(usages.outbound_available, u64::MAX);
    assert_eq!(usages.inbound_usage, 200);
    assert_eq!(usages.inbound_available, u64::MAX);
}

#[test]
fn get_rate_limit_usages_errors_when_per_eid_missing_not_global() {
    // use_global_state=false + id>0 + uninitialized per-EID => RateLimitNotInitialized.
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit_key, _) = rate_limit_pda(&fixture.oft_store, id);

    let result = fixture.context.process_instruction(&get_rate_limit_usages_ix(
        &fixture.oft_store,
        &fixture.default_rate_limit,
        &rate_limit_key,
        id,
    ));
    assert_eq!(result.program_result, program_failure(OFTError::RateLimitNotInitialized));
}

#[test]
fn get_rate_limit_usages_errors_when_only_outflow_noops_and_per_eid_missing() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit_key, _) = rate_limit_pda(&fixture.oft_store, id);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams {
                id: DEFAULT_RATE_LIMIT_ID,
                config: RateLimitConfig {
                    override_default_config: false,
                    outbound_enabled: false,
                    inbound_enabled: true,
                    net_accounting_enabled: false,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000,
                    inbound_limit: 2_000,
                    outbound_window: 60,
                    inbound_window: 60,
                },
            },
        ),
        &[Check::success()],
    );
    assert!(fixture.context.account_store.borrow().get(&rate_limit_key).is_none());
    let oft_store = read_account_data::<OFTStore>(&fixture.context, &fixture.oft_store);
    assert!(!oft_store.use_global_state);
    assert!(!oft_store.is_globally_disabled);
    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    assert!(!default_rl.config.outbound_enabled);
    assert!(default_rl.config.inbound_enabled);
    assert!(!default_rl.config.net_accounting_enabled);

    let result = fixture.context.process_instruction(&get_rate_limit_usages_ix(
        &fixture.oft_store,
        &fixture.default_rate_limit,
        &rate_limit_key,
        id,
    ));
    assert_eq!(result.program_result, program_failure(OFTError::RateLimitNotInitialized));
}

// ===================== CheckpointRateLimit: real decay =====================

#[test]
fn checkpoint_rate_limit_decays_default_state_with_advanced_clock() {
    // Override mollusk clock so `last_updated` can be seeded in the past and decay
    // becomes observable inside checkpoint.
    const CLOCK: i64 = 1_700_000_000;
    const WINDOW: u64 = 60;
    let fixture = fixture_with_clock(CLOCK);
    let clock = CLOCK as u64;

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
                    outbound_limit: 1_000,
                    inbound_limit: 1_000,
                    outbound_window: WINDOW,
                    inbound_window: WINDOW,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: 0,
                state: RateLimitState {
                    outbound_usage: 800,
                    inbound_usage: 500,
                    last_updated: clock - WINDOW,
                },
            },
        ),
        &[Check::success()],
    );

    fixture.context.process_and_validate_instruction(
        &checkpoint_rate_limit_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            DEFAULT_RATE_LIMIT_ID,
        ),
        &[Check::success()],
    );

    // elapsed = WINDOW → decay = limit * elapsed / window = 1000 → usage saturates to 0.
    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    assert_eq!(default_rl.state.outbound_usage, 0);
    assert_eq!(default_rl.state.inbound_usage, 0);
    assert_eq!(default_rl.state.last_updated, clock);
}

#[test]
fn checkpoint_rate_limit_decays_default_state_for_id_zero_when_not_global() {
    const CLOCK: i64 = 1_700_000_000;
    const WINDOW: u64 = 60;
    let fixture = fixture_with_clock(CLOCK);
    let clock = CLOCK as u64;

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams {
                id: DEFAULT_RATE_LIMIT_ID,
                config: RateLimitConfig {
                    override_default_config: false,
                    outbound_enabled: true,
                    inbound_enabled: true,
                    net_accounting_enabled: true,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000,
                    inbound_limit: 1_000,
                    outbound_window: WINDOW,
                    inbound_window: WINDOW,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: DEFAULT_RATE_LIMIT_ID,
                state: RateLimitState {
                    outbound_usage: 800,
                    inbound_usage: 500,
                    last_updated: clock - WINDOW,
                },
            },
        ),
        &[Check::success()],
    );

    fixture.context.process_and_validate_instruction(
        &checkpoint_rate_limit_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            DEFAULT_RATE_LIMIT_ID,
        ),
        &[Check::success()],
    );

    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    assert_eq!(default_rl.state.outbound_usage, 0);
    assert_eq!(default_rl.state.inbound_usage, 0);
    assert_eq!(default_rl.state.last_updated, clock);
}

#[test]
fn checkpoint_rate_limit_with_global_state_uses_default_for_per_eid_id() {
    const CLOCK: i64 = 1_700_000_000;
    const WINDOW: u64 = 60;
    let fixture = fixture_with_clock(CLOCK);
    let clock = CLOCK as u64;
    let id = DST_EID as u128;
    let (rate_limit, _) = rate_limit_pda(&fixture.oft_store, id);

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
                id: DEFAULT_RATE_LIMIT_ID,
                config: RateLimitConfig {
                    override_default_config: false,
                    outbound_enabled: true,
                    inbound_enabled: true,
                    net_accounting_enabled: true,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000,
                    inbound_limit: 1_000,
                    outbound_window: WINDOW,
                    inbound_window: WINDOW,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id: DEFAULT_RATE_LIMIT_ID,
                state: RateLimitState {
                    outbound_usage: 800,
                    inbound_usage: 500,
                    last_updated: clock - WINDOW,
                },
            },
        ),
        &[Check::success()],
    );

    fixture.context.process_and_validate_instruction(
        &checkpoint_rate_limit_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            id,
        ),
        &[Check::success()],
    );

    let default_rl = read_rate_limit(&fixture.context, &fixture.default_rate_limit);
    assert_eq!(default_rl.state.outbound_usage, 0);
    assert_eq!(default_rl.state.inbound_usage, 0);
    assert_eq!(default_rl.state.last_updated, clock);
}

#[test]
fn checkpoint_rate_limit_decays_per_eid_state_with_advanced_clock() {
    const CLOCK: i64 = 1_700_000_000;
    const WINDOW: u64 = 60;
    let fixture = fixture_with_clock(CLOCK);
    let clock = CLOCK as u64;
    let id = DST_EID as u128;
    let (rate_limit, _) = rate_limit_pda(&fixture.oft_store, id);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams {
                id,
                config: RateLimitConfig {
                    override_default_config: true,
                    outbound_enabled: true,
                    inbound_enabled: true,
                    net_accounting_enabled: true,
                    address_exemption_enabled: false,
                    outbound_limit: 1_000,
                    inbound_limit: 1_000,
                    outbound_window: WINDOW,
                    inbound_window: WINDOW,
                },
            },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_rate_limit_state_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            SetRateLimitStateParams {
                id,
                state: RateLimitState {
                    outbound_usage: 700,
                    inbound_usage: 300,
                    last_updated: clock - WINDOW / 2,
                },
            },
        ),
        &[Check::success()],
    );

    fixture.context.process_and_validate_instruction(
        &checkpoint_rate_limit_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &rate_limit,
            &fixture.rate_limiter_role_member,
            id,
        ),
        &[Check::success()],
    );

    // elapsed = WINDOW/2 = 30 → decay = 1000 * 30 / 60 = 500.
    // outbound: saturating_sub(700, 500) = 200
    // inbound:  saturating_sub(300, 500) = 0
    let rl = read_rate_limit(&fixture.context, &rate_limit);
    assert_eq!(rl.state.outbound_usage, 200);
    assert_eq!(rl.state.inbound_usage, 0);
    assert_eq!(rl.state.last_updated, clock);
}

#[test]
fn get_rate_limit_usages_reads_per_eid_when_not_global() {
    let fixture = fixture();
    let id = DST_EID as u128;
    let (rate_limit_key, _) = rate_limit_pda(&fixture.oft_store, id);

    fixture.context.process_and_validate_instruction(
        &set_rate_limit_config_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &rate_limit_key,
            &fixture.rate_limiter_role_member,
            SetRateLimitConfigParams { id, config: sample_config(777, 888) },
        ),
        &[Check::success()],
    );

    let result = fixture.context.process_and_validate_instruction(
        &get_rate_limit_usages_ix(
            &fixture.oft_store,
            &fixture.default_rate_limit,
            &rate_limit_key,
            id,
        ),
        &[Check::success()],
    );
    let usages: RateLimitUsages = decode_return(&result.return_data);
    assert_eq!(usages.outbound_available, 777);
    assert_eq!(usages.inbound_available, 888);
}
