mod oft;

use anchor_lang::error::ErrorCode;
use console_oft::{
    errors::OFTError,
    events::{DefaultPauseSet, PauseSet},
    instructions::pause::set_paused::SetPausedParams,
    state::{OFTStore, RoleType},
};
use mollusk_svm_result::Check;

use crate::oft::helpers::*;

#[test]
fn set_default_paused_rejects_missing_pauser_membership() {
    let fixture = fixture();
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::Pauser, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_default_paused_ix(
        &fixture.delegate,
        &fixture.oft_store,
        &missing_role_member,
        true,
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_default_paused_updates_store_and_rejects_idempotent_pause() {
    let fixture = fixture();

    // Inner instructions: [0] __event_cpi (DefaultPauseSet)
    let result = fixture.context.process_and_validate_instruction(
        &set_default_paused_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.pauser_role_member,
            true,
        ),
        &[Check::success(), Check::inner_instruction_count(1)],
    );

    assert_event(
        &result.inner_instructions[0].instruction.data,
        DefaultPauseSet { oft_store: to_program_pubkey(&fixture.oft_store), paused: true },
    );

    let oft_store: OFTStore = read_account(&fixture.context, &fixture.oft_store);
    assert!(oft_store.default_paused);

    let result = fixture.context.process_instruction(&set_default_paused_ix(
        &fixture.admin,
        &fixture.oft_store,
        &fixture.pauser_role_member,
        true,
    ));

    assert_eq!(result.program_result, program_failure(OFTError::PauseStateIdempotent));
}

#[test]
fn set_default_paused_allows_unpause_with_unpauser_role() {
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
        &set_default_paused_ix(
            &fixture.admin,
            &fixture.oft_store,
            &fixture.unpauser_role_member,
            false,
        ),
        &[Check::success()],
    );

    let oft_store: OFTStore = read_account(&fixture.context, &fixture.oft_store);
    assert!(!oft_store.default_paused);
}

#[test]
fn set_paused_rejects_missing_pauser_membership() {
    let fixture = fixture();
    let (pause_config, _) = pause_config_pda(&fixture.oft_store, DST_EID as u128);
    let missing_role_member =
        role_member_pda(&fixture.oft_store, RoleType::Pauser, &fixture.delegate).0;

    let result = fixture.context.process_instruction(&set_paused_ix(
        &fixture.delegate,
        &fixture.payer,
        &fixture.oft_store,
        &pause_config,
        &missing_role_member,
        SetPausedParams { id: DST_EID as u128, paused: Some(true) },
    ));

    assert_eq!(result.program_result, program_failure(ErrorCode::AccountNotInitialized));
}

#[test]
fn set_paused_creates_pause_config() {
    let fixture = fixture();
    let (pause_config, _bump) = pause_config_pda(&fixture.oft_store, DST_EID as u128);

    // Inner instructions:
    //   [0] system program create_account (init_if_needed pause_config PDA)
    //   [1] __event_cpi (PauseSet)
    let result = fixture.context.process_and_validate_instruction(
        &set_paused_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &pause_config,
            &fixture.pauser_role_member,
            SetPausedParams { id: DST_EID as u128, paused: Some(true) },
        ),
        &[
            Check::success(),
            Check::inner_instruction_count(2),
            Check::account(&pause_config).owner(&program_id()).rent_exempt().build(),
        ],
    );

    assert_event(
        &result.inner_instructions[1].instruction.data,
        PauseSet {
            oft_store: to_program_pubkey(&fixture.oft_store),
            id: DST_EID as u128,
            paused: Some(true),
        },
    );

    let config = read_pause_config(&fixture.context, &pause_config);
    assert!(config.paused);
}

#[test]
fn set_paused_allows_unpause_with_unpauser_role() {
    let fixture = fixture();
    let (pause_config, _) = pause_config_pda(&fixture.oft_store, DST_EID as u128);

    fixture.context.process_and_validate_instruction(
        &set_paused_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &pause_config,
            &fixture.pauser_role_member,
            SetPausedParams { id: DST_EID as u128, paused: Some(true) },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_paused_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &pause_config,
            &fixture.unpauser_role_member,
            SetPausedParams { id: DST_EID as u128, paused: Some(false) },
        ),
        &[Check::success()],
    );

    let config = read_pause_config(&fixture.context, &pause_config);
    assert!(!config.paused);
}

#[test]
fn set_paused_closes_override_when_disabled() {
    let fixture = fixture();
    let (pause_config, _) = pause_config_pda(&fixture.oft_store, DST_EID as u128);

    fixture.context.process_and_validate_instruction(
        &set_paused_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &pause_config,
            &fixture.pauser_role_member,
            SetPausedParams { id: DST_EID as u128, paused: Some(true) },
        ),
        &[Check::success()],
    );
    fixture.context.process_and_validate_instruction(
        &set_paused_ix(
            &fixture.admin,
            &fixture.payer,
            &fixture.oft_store,
            &pause_config,
            &fixture.unpauser_role_member,
            SetPausedParams { id: DST_EID as u128, paused: None },
        ),
        &[Check::success(), Check::account(&pause_config).closed().build()],
    );
}
