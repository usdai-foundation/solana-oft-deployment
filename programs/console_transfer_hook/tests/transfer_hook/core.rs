use crate::helpers::{
    context, default_admin_key, extra_account_meta_list_pda, hook_state_pda, init_transfer_hook_ix,
    insert_account, insert_mint, insert_wallet, mint_account_without_transfer_hook, mint_key,
    no_pending_admin, payer_key, program_err, program_id, read_account, role_member_pda, token_ix,
    transfer_hook_mint_account_with_config,
};
use anchor_lang::prelude::borsh::BorshDeserialize;
use console_transfer_hook::{
    errors::HookError, state::TransferHookState, AllowlistMode, RoleMember, RoleType,
};
use mollusk_svm::result::Check;

#[test]
fn init_transfer_hook_bootstraps_mint_state_and_default_admin() {
    let context = context();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let mint = mint_key();
    let (hook_state, hook_state_bump) = hook_state_pda(&mint);
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(&mint);
    let (admin_role_member, admin_role_bump) =
        role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    insert_wallet(&context, payer);
    insert_wallet(&context, transfer_hook_authority);
    insert_wallet(&context, admin);
    insert_mint(&context, &mint);

    context.process_and_validate_instruction(
        &init_transfer_hook_ix(
            &payer,
            &transfer_hook_authority,
            &mint,
            &extra_account_meta_list,
            &hook_state,
            &admin_role_member,
            &admin,
            false,
            AllowlistMode::Open,
        ),
        &[
            Check::success(),
            Check::account(&hook_state).owner(&program_id()).rent_exempt().build(),
            Check::account(&extra_account_meta_list)
                .owner(&program_id())
                .rent_exempt()
                .build(),
            Check::account(&admin_role_member).owner(&program_id()).rent_exempt().build(),
        ],
    );

    let config: TransferHookState = read_account(&context, &hook_state);
    assert_eq!(config.mint, crate::helpers::to_program_pubkey(&mint));
    assert!(!config.paused);
    assert_eq!(config.allowlist_mode, AllowlistMode::Open);
    assert_eq!(config.current_default_admin, crate::helpers::to_program_pubkey(&admin));
    assert_eq!(
        config.pending_default_admin,
        crate::helpers::to_program_pubkey(&no_pending_admin())
    );
    assert_eq!(config.bump, hook_state_bump);

    let role_member: RoleMember = read_account(&context, &admin_role_member);
    assert!(role_member.role == RoleType::DefaultAdmin);
    assert_eq!(role_member.account, crate::helpers::to_program_pubkey(&admin));
    assert_eq!(role_member.bump, admin_role_bump);
}

#[test]
fn token_view_returns_configured_mint() {
    let context = context();
    let (_payer, _transfer_hook_authority, _admin, mint, hook_state) =
        crate::helpers::init_default_fixture(&context, AllowlistMode::Open, false);

    let result = context.process_instruction(&token_ix(&hook_state));

    assert!(result.program_result.is_ok());
    let actual = anchor_lang::prelude::Pubkey::try_from_slice(&result.return_data)
        .expect("decode token mint");
    assert_eq!(actual, crate::helpers::to_program_pubkey(&mint));
}

#[test]
fn init_transfer_hook_requires_transfer_hook_authority() {
    let context = context();
    let payer = payer_key();
    let admin = default_admin_key();
    let invalid_transfer_hook_authority = crate::helpers::blacklister_key();
    let mint = mint_key();
    let (hook_state, _) = hook_state_pda(&mint);
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(&mint);
    let (admin_role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    insert_wallet(&context, payer);
    insert_wallet(&context, invalid_transfer_hook_authority);
    insert_mint(&context, &mint);

    context.process_and_validate_instruction(
        &init_transfer_hook_ix(
            &payer,
            &invalid_transfer_hook_authority,
            &mint,
            &extra_account_meta_list,
            &hook_state,
            &admin_role_member,
            &admin,
            false,
            AllowlistMode::Open,
        ),
        &[Check::err(program_err(HookError::InvalidTransferHookAuthority))],
    );
}

#[test]
fn init_transfer_hook_validates_transfer_hook_extension_state() {
    let cases = [
        (mint_account_without_transfer_hook(9), HookError::MissingTransferHookExtension),
        (
            transfer_hook_mint_account_with_config(9, Some(default_admin_key()), None, None),
            HookError::InvalidTransferHookProgram,
        ),
        (
            transfer_hook_mint_account_with_config(
                9,
                Some(default_admin_key()),
                Some(crate::helpers::blacklister_key()),
                None,
            ),
            HookError::InvalidTransferHookProgram,
        ),
        (
            transfer_hook_mint_account_with_config(9, None, Some(program_id()), None),
            HookError::InvalidTransferHookAuthority,
        ),
    ];

    for (mint_account, expected_error) in cases {
        let context = context();
        let payer = payer_key();
        let transfer_hook_authority = default_admin_key();
        let admin = default_admin_key();
        let mint = mint_key();
        let (hook_state, _) = hook_state_pda(&mint);
        let (extra_account_meta_list, _) = extra_account_meta_list_pda(&mint);
        let (admin_role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

        insert_wallet(&context, payer);
        insert_wallet(&context, transfer_hook_authority);
        insert_account(&context, mint, mint_account);

        context.process_and_validate_instruction(
            &init_transfer_hook_ix(
                &payer,
                &transfer_hook_authority,
                &mint,
                &extra_account_meta_list,
                &hook_state,
                &admin_role_member,
                &admin,
                false,
                AllowlistMode::Open,
            ),
            &[Check::err(program_err(expected_error))],
        );
    }
}
