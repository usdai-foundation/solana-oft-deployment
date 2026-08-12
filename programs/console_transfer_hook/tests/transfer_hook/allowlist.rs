use crate::helpers::{
    allowlist_entry_pda, blacklister_key, context, init_default_fixture, insert_account,
    insert_allowlist_entry, insert_role_member, insert_wallet, is_allowlisted_ix,
    is_blacklisted_ix, is_whitelisted_ix, program_err, read_account, role_member_pda,
    set_allowlist_mode_ix, set_allowlist_mode_raw_ix, set_blacklisted_ix, set_whitelisted_ix,
    user_key, whitelister_key, SYSTEM_PROGRAM,
};
use anchor_lang::AnchorDeserialize;
use console_transfer_hook::{
    errors::HookError,
    state::{AllowlistEntry, TransferHookState},
    AllowlistMode, RoleType,
};
use mollusk_svm::result::Check;
use solana_account::Account;

#[test]
fn set_allowlist_mode_updates_hook_state() {
    let context = context();
    let (payer, _transfer_hook_authority, admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let (role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    insert_wallet(&context, payer);

    context.process_and_validate_instruction(
        &set_allowlist_mode_ix(&admin, &hook_state, &role_member, AllowlistMode::Blacklist),
        &[Check::success()],
    );

    let config: TransferHookState = read_account(&context, &hook_state);
    assert_eq!(config.allowlist_mode, AllowlistMode::Blacklist);
}

#[test]
fn set_allowlist_mode_rejects_idempotent_and_invalid_updates() {
    let context = context();
    let (_payer, _transfer_hook_authority, admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let (role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    context.process_and_validate_instruction(
        &set_allowlist_mode_ix(&admin, &hook_state, &role_member, AllowlistMode::Open),
        &[Check::err(program_err(HookError::ModeAlreadySet))],
    );
    // Garbage discriminant byte is rejected by Anchor at borsh deserialization
    // (InstructionDidNotDeserialize = 102), before our handler ever runs.
    context.process_and_validate_instruction(
        &set_allowlist_mode_raw_ix(&admin, &hook_state, &role_member, 42),
        &[Check::err(solana_program_error::ProgramError::Custom(102))],
    );
}

#[test]
fn set_allowlist_mode_rejects_missing_default_admin_membership() {
    let context = context();
    let (_payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let authority = whitelister_key();
    let (role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &authority);

    insert_wallet(&context, authority);

    let result = context.process_instruction(&set_allowlist_mode_ix(
        &authority,
        &hook_state,
        &role_member,
        AllowlistMode::Blacklist,
    ));
    assert!(result.program_result.is_err());
}

#[test]
fn blacklist_entries_are_hook_state_scoped_and_used_by_views() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Blacklist, false);
    let authority = blacklister_key();
    let user = user_key();
    let role_member = insert_role_member(&context, &hook_state, RoleType::Blacklister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &role_member,
            &user,
            true,
        ),
        &[Check::success()],
    );

    let listed = context.process_and_validate_instruction(
        &is_blacklisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(bool::try_from_slice(&listed.return_data).expect("decode bool"));

    let allowed = context.process_and_validate_instruction(
        &is_allowlisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(!bool::try_from_slice(&allowed.return_data).expect("decode bool"));

    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &role_member,
            &user,
            false,
        ),
        &[Check::success(), Check::account(&allowlist_entry).closed().build()],
    );

    let unlisted = context.process_and_validate_instruction(
        &is_blacklisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(!bool::try_from_slice(&unlisted.return_data).expect("decode bool"));
}

#[test]
fn set_blacklisted_handles_prefunded_entry_pda() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Blacklist, false);
    let authority = blacklister_key();
    let user = user_key();
    let role_member = insert_role_member(&context, &hook_state, RoleType::Blacklister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);
    insert_account(&context, allowlist_entry, Account::new(1, 0, &SYSTEM_PROGRAM));

    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &role_member,
            &user,
            true,
        ),
        &[
            Check::success(),
            Check::account(&allowlist_entry)
                .owner(&crate::helpers::program_id())
                .rent_exempt()
                .build(),
        ],
    );

    let entry: AllowlistEntry = read_account(&context, &allowlist_entry);
    assert!(entry.is_blacklisted);
    assert!(!entry.is_whitelisted);
}

#[test]
fn set_blacklisted_rejects_missing_blacklister_membership() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let authority = blacklister_key();
    let user = user_key();
    let (role_member, _) = role_member_pda(&hook_state, RoleType::Blacklister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    let result = context.process_instruction(&set_blacklisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        true,
    ));
    assert!(result.program_result.is_err());
}

#[test]
fn set_blacklisted_rejects_noop_updates() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Blacklist, false);
    let authority = blacklister_key();
    let user = user_key();
    let role_member = insert_role_member(&context, &hook_state, RoleType::Blacklister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    let ix = set_blacklisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        true,
    );
    context.process_and_validate_instruction(&ix, &[Check::success()]);
    context.process_and_validate_instruction(
        &ix,
        &[Check::err(program_err(HookError::AllowlistStateIdempotent))],
    );

    let ix = set_blacklisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        false,
    );
    context.process_and_validate_instruction(&ix, &[Check::success()]);
    context.process_and_validate_instruction(
        &ix,
        &[
            Check::err(program_err(HookError::AllowlistStateIdempotent)),
            Check::account(&allowlist_entry).data(&[]).build(),
        ],
    );
}

#[test]
fn whitelist_entries_are_hook_state_scoped_and_used_by_views() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Whitelist, false);
    let authority = whitelister_key();
    let user = user_key();
    let role_member = insert_role_member(&context, &hook_state, RoleType::Whitelister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &role_member,
            &user,
            true,
        ),
        &[
            Check::success(),
            Check::account(&allowlist_entry)
                .owner(&crate::helpers::program_id())
                .rent_exempt()
                .build(),
        ],
    );

    let listed = context.process_and_validate_instruction(
        &is_whitelisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(bool::try_from_slice(&listed.return_data).expect("decode bool"));

    let allowed = context.process_and_validate_instruction(
        &is_allowlisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(bool::try_from_slice(&allowed.return_data).expect("decode bool"));

    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &role_member,
            &user,
            false,
        ),
        &[Check::success(), Check::account(&allowlist_entry).closed().build()],
    );

    let unlisted = context.process_and_validate_instruction(
        &is_whitelisted_ix(&hook_state, &user, &allowlist_entry),
        &[Check::success()],
    );
    assert!(!bool::try_from_slice(&unlisted.return_data).expect("decode bool"));
}

#[test]
fn set_whitelisted_rejects_missing_whitelister_membership() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Open, false);
    let authority = whitelister_key();
    let user = user_key();
    let (role_member, _) = role_member_pda(&hook_state, RoleType::Whitelister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    let result = context.process_instruction(&set_whitelisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        true,
    ));
    assert!(result.program_result.is_err());
}

#[test]
fn set_whitelisted_rejects_noop_updates() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Whitelist, false);
    let authority = whitelister_key();
    let user = user_key();
    let role_member = insert_role_member(&context, &hook_state, RoleType::Whitelister, &authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, authority);

    let ix = set_whitelisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        true,
    );
    context.process_and_validate_instruction(&ix, &[Check::success()]);
    context.process_and_validate_instruction(
        &ix,
        &[Check::err(program_err(HookError::AllowlistStateIdempotent))],
    );

    let ix = set_whitelisted_ix(
        &authority,
        &payer,
        &hook_state,
        &allowlist_entry,
        &role_member,
        &user,
        false,
    );
    context.process_and_validate_instruction(&ix, &[Check::success()]);
    context.process_and_validate_instruction(
        &ix,
        &[
            Check::err(program_err(HookError::AllowlistStateIdempotent)),
            Check::account(&allowlist_entry).data(&[]).build(),
        ],
    );
}

#[test]
fn is_allowlisted_matches_mode_and_marker_state() {
    let cases = [
        (AllowlistMode::Open, true, false, true),
        (AllowlistMode::Blacklist, false, false, true),
        (AllowlistMode::Blacklist, true, false, false),
        (AllowlistMode::Whitelist, false, false, false),
        (AllowlistMode::Whitelist, false, true, true),
    ];

    for (mode, blacklisted, whitelisted, expected) in cases {
        let context = context();
        let (_payer, _transfer_hook_authority, _admin, _mint, hook_state) =
            init_default_fixture(&context, mode, false);
        let user = user_key();
        let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

        if blacklisted || whitelisted {
            insert_allowlist_entry(&context, &hook_state, &user, blacklisted, whitelisted);
        }

        let result = context.process_and_validate_instruction(
            &is_allowlisted_ix(&hook_state, &user, &allowlist_entry),
            &[Check::success()],
        );
        let actual = bool::try_from_slice(&result.return_data).expect("decode bool");
        assert_eq!(
            actual, expected,
            "mode={mode:?} blacklisted={blacklisted} whitelisted={whitelisted}"
        );
    }
}

#[test]
fn is_allowlisted_resolves_dual_entries_by_mode() {
    let cases = [
        (AllowlistMode::Blacklist, false),
        (AllowlistMode::Whitelist, true),
        (AllowlistMode::Open, true),
    ];

    for (mode, expected) in cases {
        let context = context();
        let (_payer, _transfer_hook_authority, _admin, _mint, hook_state) =
            init_default_fixture(&context, mode, false);
        let user = user_key();
        let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

        insert_allowlist_entry(&context, &hook_state, &user, true, true);

        let result = context.process_and_validate_instruction(
            &is_allowlisted_ix(&hook_state, &user, &allowlist_entry),
            &[Check::success()],
        );
        let actual = bool::try_from_slice(&result.return_data).expect("decode bool");
        assert_eq!(actual, expected, "mode={mode:?}");
    }
}

#[test]
fn set_blacklisted_then_whitelisted_keeps_single_pda_with_both_flags() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Blacklist, false);
    let bl_authority = blacklister_key();
    let wl_authority = whitelister_key();
    let user = user_key();
    let bl_role = insert_role_member(&context, &hook_state, RoleType::Blacklister, &bl_authority);
    let wl_role = insert_role_member(&context, &hook_state, RoleType::Whitelister, &wl_authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, bl_authority);
    insert_wallet(&context, wl_authority);

    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &bl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &bl_role,
            &user,
            true,
        ),
        &[Check::success()],
    );
    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &wl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &wl_role,
            &user,
            true,
        ),
        &[
            Check::success(),
            Check::account(&allowlist_entry)
                .owner(&crate::helpers::program_id())
                .rent_exempt()
                .build(),
        ],
    );

    let entry: AllowlistEntry = read_account(&context, &allowlist_entry);
    assert!(entry.is_blacklisted);
    assert!(entry.is_whitelisted);
}

#[test]
fn clearing_one_flag_keeps_pda_open_while_other_flag_is_set() {
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Blacklist, false);
    let bl_authority = blacklister_key();
    let wl_authority = whitelister_key();
    let user = user_key();
    let bl_role = insert_role_member(&context, &hook_state, RoleType::Blacklister, &bl_authority);
    let wl_role = insert_role_member(&context, &hook_state, RoleType::Whitelister, &wl_authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, bl_authority);
    insert_wallet(&context, wl_authority);

    // Build up the (true, true) state through the program so the runtime
    // owns rent_epoch (Check::closed compares against Account::default()).
    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &bl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &bl_role,
            &user,
            true,
        ),
        &[Check::success()],
    );
    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &wl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &wl_role,
            &user,
            true,
        ),
        &[Check::success()],
    );

    // Clear blacklist — whitelist still set, PDA must stay open.
    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &bl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &bl_role,
            &user,
            false,
        ),
        &[Check::success()],
    );
    let entry: AllowlistEntry = read_account(&context, &allowlist_entry);
    assert!(!entry.is_blacklisted);
    assert!(entry.is_whitelisted);

    // Clear whitelist — both flags now false, PDA must close.
    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &wl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &wl_role,
            &user,
            false,
        ),
        &[Check::success(), Check::account(&allowlist_entry).closed().build()],
    );
}

#[test]
fn set_blacklisted_true_succeeds_when_only_whitelist_flag_is_set() {
    // Per-flag idempotency: setBlacklisted(true) must NOT revert as idempotent
    // when only whitelist is currently true.
    let context = context();
    let (payer, _transfer_hook_authority, _admin, _mint, hook_state) =
        init_default_fixture(&context, AllowlistMode::Whitelist, false);
    let bl_authority = blacklister_key();
    let wl_authority = whitelister_key();
    let user = user_key();
    let bl_role = insert_role_member(&context, &hook_state, RoleType::Blacklister, &bl_authority);
    let wl_role = insert_role_member(&context, &hook_state, RoleType::Whitelister, &wl_authority);
    let (allowlist_entry, _) = allowlist_entry_pda(&hook_state, &user);

    insert_wallet(&context, payer);
    insert_wallet(&context, bl_authority);
    insert_wallet(&context, wl_authority);

    context.process_and_validate_instruction(
        &set_whitelisted_ix(
            &wl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &wl_role,
            &user,
            true,
        ),
        &[Check::success()],
    );

    context.process_and_validate_instruction(
        &set_blacklisted_ix(
            &bl_authority,
            &payer,
            &hook_state,
            &allowlist_entry,
            &bl_role,
            &user,
            true,
        ),
        &[Check::success()],
    );

    let entry: AllowlistEntry = read_account(&context, &allowlist_entry);
    assert!(entry.is_blacklisted);
    assert!(entry.is_whitelisted);
}
