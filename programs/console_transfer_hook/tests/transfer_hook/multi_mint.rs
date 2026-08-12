use crate::helpers::{
    allowlist_entry_pda, bypass_account_key, bypass_entry_pda, bypass_manager_key,
    default_admin_key, expect_program_failure, hook_state_pda, init_transfer_hook_ix,
    insert_account, insert_mint, insert_role_member, insert_wallet, pause_ix, pauser_key,
    payer_key, process_and_validate_transaction_instructions, read_account, role_member_pda,
    set_blacklisted_ix, set_bypass_ix, transfer_checked_with_hook_ix, transfer_hook_token_account,
    LIVE_TRANSFER_AMOUNT, LIVE_TRANSFER_DECIMALS, LIVE_TRANSFER_STARTING_BALANCE,
};
use console_transfer_hook::{errors::HookError, state::TransferHookState, AllowlistMode, RoleType};
use mollusk_svm::result::Check;
use solana_pubkey::Pubkey;

fn init_transfer_hook_for_test(
    context: &crate::helpers::TestContext,
    payer: &Pubkey,
    transfer_hook_authority: &Pubkey,
    admin: &Pubkey,
    mint: &Pubkey,
    allowlist_mode: AllowlistMode,
    paused: bool,
) -> Pubkey {
    insert_wallet(context, *payer);
    insert_mint(context, mint);
    let (hook_state, _) = hook_state_pda(mint);
    let (extra, _) = crate::helpers::extra_account_meta_list_pda(mint);
    let (admin_role, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, admin);

    context.process_and_validate_instruction(
        &init_transfer_hook_ix(
            payer,
            transfer_hook_authority,
            mint,
            &extra,
            &hook_state,
            &admin_role,
            admin,
            paused,
            allowlist_mode,
        ),
        &[Check::success()],
    );

    hook_state
}

fn insert_transfer_accounts(
    context: &crate::helpers::TestContext,
    mint: &Pubkey,
    marker: u8,
) -> (Pubkey, Pubkey, Pubkey) {
    let source_owner = Pubkey::new_from_array([marker; 32]);
    let source_token = Pubkey::new_from_array([marker.wrapping_add(1); 32]);
    let destination_token = Pubkey::new_from_array([marker.wrapping_add(2); 32]);

    insert_wallet(context, source_owner);
    insert_account(
        context,
        source_token,
        transfer_hook_token_account(mint, &source_owner, LIVE_TRANSFER_STARTING_BALANCE),
    );
    insert_account(
        context,
        destination_token,
        transfer_hook_token_account(mint, &Pubkey::new_from_array([marker.wrapping_add(3); 32]), 0),
    );

    (source_owner, source_token, destination_token)
}

#[test]
fn allowlist_entries_do_not_cross_hook_state_namespaces() {
    let context = crate::helpers::context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let authority = crate::helpers::blacklister_key();
    let user = crate::helpers::user_key();
    let mint_a = Pubkey::new_from_array([0xCA; 32]);
    let mint_b = Pubkey::new_from_array([0xCB; 32]);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_wallet(&context, authority);
    insert_mint(&context, &mint_a);
    insert_mint(&context, &mint_b);

    let mut configs = Vec::new();
    for mint in [mint_a, mint_b] {
        insert_wallet(&context, payer);
        let (hook_state, _) = hook_state_pda(&mint);
        let (extra, _) = crate::helpers::extra_account_meta_list_pda(&mint);
        let (admin_role, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);
        context.process_and_validate_instruction(
            &init_transfer_hook_ix(
                &payer,
                &transfer_hook_authority,
                &mint,
                &extra,
                &hook_state,
                &admin_role,
                &admin,
                false,
                AllowlistMode::Blacklist,
            ),
            &[Check::success()],
        );
        configs.push((mint, hook_state));
    }

    let role_member =
        insert_role_member(&context, &configs[0].1, RoleType::Blacklister, &authority);
    let (entry_a, _) = allowlist_entry_pda(&configs[0].1, &user);
    let (entry_b, _) = allowlist_entry_pda(&configs[1].1, &user);

    context.process_and_validate_instruction(
        &set_blacklisted_ix(&authority, &payer, &configs[0].1, &entry_a, &role_member, &user, true),
        &[Check::success()],
    );

    assert_ne!(entry_a, entry_b);
    assert!(crate::helpers::account_exists(&context, &entry_a));
    assert!(!crate::helpers::account_exists(&context, &entry_b));
}

#[test]
fn blacklister_role_does_not_cross_hook_state_namespaces() {
    let context = crate::helpers::context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let authority = crate::helpers::blacklister_key();
    let user = crate::helpers::user_key();
    let mint_a = Pubkey::new_from_array([0xD0; 32]);
    let mint_b = Pubkey::new_from_array([0xD1; 32]);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_wallet(&context, authority);

    let hook_state_a = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_a,
        AllowlistMode::Blacklist,
        false,
    );
    let hook_state_b = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_b,
        AllowlistMode::Blacklist,
        false,
    );

    let role_member_a =
        insert_role_member(&context, &hook_state_a, RoleType::Blacklister, &authority);
    let (entry_b, _) = allowlist_entry_pda(&hook_state_b, &user);

    let result = context.process_instruction(&set_blacklisted_ix(
        &authority,
        &payer,
        &hook_state_b,
        &entry_b,
        &role_member_a,
        &user,
        true,
    ));
    assert!(result.program_result.is_err());
    assert!(!crate::helpers::account_exists(&context, &entry_b));
}

#[test]
fn bypass_entries_do_not_cross_hook_state_namespaces() {
    let context = crate::helpers::context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let authority = bypass_manager_key();
    let account = bypass_account_key();
    let mint_a = Pubkey::new_from_array([0xCC; 32]);
    let mint_b = Pubkey::new_from_array([0xCD; 32]);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_wallet(&context, authority);
    insert_mint(&context, &mint_a);
    insert_mint(&context, &mint_b);

    let mut configs = Vec::new();
    for mint in [mint_a, mint_b] {
        insert_wallet(&context, payer);
        let (hook_state, _) = hook_state_pda(&mint);
        let (extra, _) = crate::helpers::extra_account_meta_list_pda(&mint);
        let (admin_role, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);
        context.process_and_validate_instruction(
            &init_transfer_hook_ix(
                &payer,
                &transfer_hook_authority,
                &mint,
                &extra,
                &hook_state,
                &admin_role,
                &admin,
                false,
                AllowlistMode::Open,
            ),
            &[Check::success()],
        );
        configs.push((mint, hook_state));
    }

    let role_member =
        insert_role_member(&context, &configs[0].1, RoleType::BypassManager, &authority);
    let (entry_a, _) = bypass_entry_pda(&configs[0].1, &account);
    let (entry_b, _) = bypass_entry_pda(&configs[1].1, &account);

    insert_wallet(&context, payer);
    context.process_and_validate_instruction(
        &set_bypass_ix(&payer, &authority, &configs[0].1, &entry_a, &role_member, &account, true),
        &[Check::success()],
    );

    assert_ne!(entry_a, entry_b);
    assert!(crate::helpers::account_exists(&context, &entry_a));
    assert!(!crate::helpers::account_exists(&context, &entry_b));
}

#[test]
fn pause_state_does_not_cross_hook_state_namespaces() {
    let context = crate::helpers::context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let pauser = pauser_key();
    let mint_a = Pubkey::new_from_array([0xD2; 32]);
    let mint_b = Pubkey::new_from_array([0xD3; 32]);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_wallet(&context, pauser);

    let hook_state_a = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_a,
        AllowlistMode::Open,
        false,
    );
    let hook_state_b = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_b,
        AllowlistMode::Open,
        false,
    );

    let pauser_role = insert_role_member(&context, &hook_state_a, RoleType::Pauser, &pauser);
    context.process_and_validate_instruction(
        &pause_ix(&pauser, &hook_state_a, &pauser_role),
        &[Check::success()],
    );

    let config_a: TransferHookState = read_account(&context, &hook_state_a);
    let config_b: TransferHookState = read_account(&context, &hook_state_b);
    assert!(config_a.paused);
    assert!(!config_b.paused);

    let (source_owner_a, source_token_a, destination_token_a) =
        insert_transfer_accounts(&context, &mint_a, 0xD4);
    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &source_token_a,
        &mint_a,
        &destination_token_a,
        &source_owner_a,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::Paused);

    let (source_owner_b, source_token_b, destination_token_b) =
        insert_transfer_accounts(&context, &mint_b, 0xD8);
    process_and_validate_transaction_instructions(
        &context,
        &[transfer_checked_with_hook_ix(
            &context,
            &source_token_b,
            &mint_b,
            &destination_token_b,
            &source_owner_b,
            LIVE_TRANSFER_AMOUNT,
            LIVE_TRANSFER_DECIMALS,
        )],
        &[Check::success()],
    );
}

#[test]
fn source_bypass_does_not_cross_hook_state_namespaces_in_transfer_hook() {
    let context = crate::helpers::context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let authority = bypass_manager_key();
    let mint_a = Pubkey::new_from_array([0xDC; 32]);
    let mint_b = Pubkey::new_from_array([0xDD; 32]);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_wallet(&context, authority);

    let hook_state_a = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_a,
        AllowlistMode::Open,
        false,
    );
    let hook_state_b = init_transfer_hook_for_test(
        &context,
        &payer,
        &transfer_hook_authority,
        &admin,
        &mint_b,
        AllowlistMode::Open,
        true,
    );

    let (source_owner_b, source_token_b, destination_token_b) =
        insert_transfer_accounts(&context, &mint_b, 0xE0);
    let role_member_a =
        insert_role_member(&context, &hook_state_a, RoleType::BypassManager, &authority);
    let (bypass_entry_a, _) = bypass_entry_pda(&hook_state_a, &source_token_b);
    let (bypass_entry_b, _) = bypass_entry_pda(&hook_state_b, &source_token_b);

    context.process_and_validate_instruction(
        &set_bypass_ix(
            &payer,
            &authority,
            &hook_state_a,
            &bypass_entry_a,
            &role_member_a,
            &source_token_b,
            true,
        ),
        &[Check::success()],
    );

    assert!(crate::helpers::account_exists(&context, &bypass_entry_a));
    assert!(!crate::helpers::account_exists(&context, &bypass_entry_b));

    let result = context.process_instruction(&transfer_checked_with_hook_ix(
        &context,
        &source_token_b,
        &mint_b,
        &destination_token_b,
        &source_owner_b,
        LIVE_TRANSFER_AMOUNT,
        LIVE_TRANSFER_DECIMALS,
    ));
    expect_program_failure(&result, HookError::Paused);
}
