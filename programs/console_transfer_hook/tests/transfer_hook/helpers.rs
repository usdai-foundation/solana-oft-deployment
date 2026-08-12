#![allow(dead_code)]

use anchor_lang::{prelude::borsh, AccountDeserialize, AnchorSerialize, Discriminator};
use console_transfer_hook::{
    errors::HookError,
    instruction as ix,
    state::{
        AllowlistEntry, ALLOWLIST_SEED, BYPASS_SEED, EXTRA_ACCOUNT_METAS_SEED, TRANSFER_HOOK_SEED,
    },
    transfer_hook_info::TRANSFER_HOOK_INFO_SEED,
    AllowlistMode, InitTransferHookParams, RoleMember, RoleType, SetAllowlistedParams,
    SetBypassParams,
};
use futures::executor::block_on;
use mollusk_svm::{account_store::AccountStore, Mollusk, MolluskContext};
use mollusk_svm_programs_token::token2022;
use mollusk_svm_result::types::{TransactionProgramResult, TransactionResult};
use rbac::ROLE_MEMBER_SEED;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;
use spl_pod::{
    optional_keys::OptionalNonZeroPubkey,
    primitives::{PodBool, PodU64},
};
use spl_token_2022::{
    extension::{
        permanent_delegate::PermanentDelegate,
        transfer_hook::{TransferHook, TransferHookAccount},
        BaseStateWithExtensionsMut, ExtensionType, PodStateWithExtensions,
        PodStateWithExtensionsMut,
    },
    offchain::create_transfer_checked_instruction_with_extra_metas,
    pod::{PodAccount, PodCOption, PodMint},
    state::AccountState,
};
use std::{
    collections::{HashMap, HashSet},
    future::ready,
    sync::Once,
};

pub const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([0u8; 32]);
static QUIET_MOLLUSK_LOGS: Once = Once::new();

pub type TestContext = MolluskContext<HashMap<Pubkey, Account>>;

pub const LIVE_TRANSFER_DECIMALS: u8 = 9;
pub const LIVE_TRANSFER_AMOUNT: u64 = 50;
pub const LIVE_TRANSFER_STARTING_BALANCE: u64 = 1_000_000;

pub struct LiveTransferFixture {
    pub payer: Pubkey,
    pub transfer_hook_authority: Pubkey,
    pub admin: Pubkey,
    pub hook_state: Pubkey,
    pub mint: Pubkey,
    pub source_owner: Pubkey,
    pub source_token: Pubkey,
    pub destination_token: Pubkey,
}

pub fn program_id() -> Pubkey {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(console_transfer_hook::ID.as_ref());
    Pubkey::new_from_array(bytes)
}

pub fn to_program_pubkey(key: &Pubkey) -> anchor_lang::prelude::Pubkey {
    anchor_lang::prelude::Pubkey::new_from_array(*key.as_array())
}

pub fn assert_event<T: AnchorSerialize + Discriminator>(data: &[u8], expected: T) {
    assert_eq!(&data[8..16], T::DISCRIMINATOR, "event discriminator mismatch");
    let expected_bytes = borsh::to_vec(&expected).expect("serialize expected");
    assert_eq!(&data[16..], expected_bytes.as_slice(), "event payload mismatch");
}

pub fn payer_key() -> Pubkey {
    Pubkey::new_from_array([0xAA; 32])
}

pub fn default_admin_key() -> Pubkey {
    Pubkey::new_from_array([0xAB; 32])
}

pub fn blacklister_key() -> Pubkey {
    Pubkey::new_from_array([0xAE; 32])
}

pub fn whitelister_key() -> Pubkey {
    Pubkey::new_from_array([0xAF; 32])
}

pub fn pauser_key() -> Pubkey {
    Pubkey::new_from_array([0xB0; 32])
}

pub fn unpauser_key() -> Pubkey {
    Pubkey::new_from_array([0xB1; 32])
}

pub fn bypass_manager_key() -> Pubkey {
    Pubkey::new_from_array([0xB2; 32])
}

pub fn user_key() -> Pubkey {
    Pubkey::new_from_array([0xBB; 32])
}

pub fn mint_key() -> Pubkey {
    Pubkey::new_from_array([0xBC; 32])
}

pub fn bypass_account_key() -> Pubkey {
    Pubkey::new_from_array([0xBD; 32])
}

pub fn transfer_owner_key() -> Pubkey {
    Pubkey::new_from_array([0xBE; 32])
}

pub fn transfer_recipient_key() -> Pubkey {
    Pubkey::new_from_array([0xBF; 32])
}

pub fn source_token_account_key() -> Pubkey {
    Pubkey::new_from_array([0xC0; 32])
}

pub fn destination_token_account_key() -> Pubkey {
    Pubkey::new_from_array([0xC1; 32])
}

pub fn no_pending_admin() -> Pubkey {
    Pubkey::new_from_array([0u8; 32])
}

pub fn event_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"__event_authority"], &program_id())
}

pub fn role_member_pda(state_key: &Pubkey, role: RoleType, member: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ROLE_MEMBER_SEED, state_key.as_ref(), &[role as u8], member.as_ref()],
        &program_id(),
    )
}

pub fn hook_state_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TRANSFER_HOOK_SEED, mint.as_ref()], &program_id())
}

pub fn extra_account_meta_list_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[EXTRA_ACCOUNT_METAS_SEED, mint.as_ref()], &program_id())
}

pub fn transfer_hook_info_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TRANSFER_HOOK_INFO_SEED], &program_id())
}

pub fn allowlist_entry_pda(hook_state: &Pubkey, pubkey: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ALLOWLIST_SEED, hook_state.as_ref(), pubkey.as_ref()],
        &program_id(),
    )
}

pub fn bypass_entry_pda(hook_state: &Pubkey, account: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[BYPASS_SEED, hook_state.as_ref(), account.as_ref()],
        &program_id(),
    )
}

pub fn mollusk() -> Mollusk {
    quiet_mollusk_logs();
    let binary = target_program_path();
    let mut mollusk = Mollusk::new(&program_id(), binary.to_str().unwrap());
    token2022::add_program(&mut mollusk);
    mollusk
}

fn quiet_mollusk_logs() {
    QUIET_MOLLUSK_LOGS.call_once(|| {
        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var("RUST_LOG", "off");
        }
    });
}

pub fn context() -> TestContext {
    mollusk().with_context(HashMap::new())
}

fn target_program_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("Cargo.lock").exists())
        .expect("Cargo.lock not found")
        .join("target/deploy/console_transfer_hook")
}

pub fn ix_data<T: AnchorSerialize>(discriminator: &[u8], params: T) -> Vec<u8> {
    let mut data = discriminator.to_vec();
    data.extend(borsh::to_vec(&params).unwrap());
    data
}

pub fn wallet_account() -> Account {
    Account::new(10_000_000, 0, &SYSTEM_PROGRAM)
}

pub fn transfer_hook_mint_account_with_delegate(
    decimals: u8,
    permanent_delegate: Option<Pubkey>,
) -> Account {
    transfer_hook_mint_account_with_config(
        decimals,
        Some(default_admin_key()),
        Some(program_id()),
        permanent_delegate,
    )
}

pub fn transfer_hook_mint_account_with_config(
    decimals: u8,
    transfer_hook_authority: Option<Pubkey>,
    transfer_hook_program: Option<Pubkey>,
    permanent_delegate: Option<Pubkey>,
) -> Account {
    let mut extension_types = vec![ExtensionType::TransferHook];
    if permanent_delegate.is_some() {
        extension_types.push(ExtensionType::PermanentDelegate);
    }

    let mint_len = ExtensionType::try_calculate_account_len::<PodMint>(&extension_types)
        .expect("calculate transfer hook mint len");
    let mut data = vec![0u8; mint_len];
    let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data).unwrap();
    let extension = mint.init_extension::<TransferHook>(true).expect("init transfer hook ext");
    extension.authority = OptionalNonZeroPubkey::try_from(transfer_hook_authority).unwrap();
    extension.program_id = OptionalNonZeroPubkey::try_from(transfer_hook_program).unwrap();
    if let Some(delegate) = permanent_delegate {
        let extension = mint
            .init_extension::<PermanentDelegate>(true)
            .expect("init permanent delegate ext");
        extension.delegate = OptionalNonZeroPubkey::try_from(Some(delegate)).unwrap();
    }

    mint.base.mint_authority = PodCOption::some(default_admin_key());
    mint.base.supply = PodU64::from(1_000_000);
    mint.base.decimals = decimals;
    mint.base.is_initialized = PodBool::from_bool(true);
    mint.base.freeze_authority = PodCOption::none();
    mint.init_account_type().expect("init mint account type");

    Account {
        lamports: 1_000_000,
        data,
        owner: token2022::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn mint_account_without_transfer_hook(decimals: u8) -> Account {
    let mint_len =
        ExtensionType::try_calculate_account_len::<PodMint>(&[]).expect("calculate mint len");
    let mut data = vec![0u8; mint_len];
    let mut mint = PodStateWithExtensionsMut::<PodMint>::unpack_uninitialized(&mut data).unwrap();

    mint.base.mint_authority = PodCOption::some(default_admin_key());
    mint.base.supply = PodU64::from(1_000_000);
    mint.base.decimals = decimals;
    mint.base.is_initialized = PodBool::from_bool(true);
    mint.base.freeze_authority = PodCOption::none();
    mint.init_account_type().expect("init mint account type");

    Account {
        lamports: 1_000_000,
        data,
        owner: token2022::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn transfer_hook_token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    let extension_types =
        ExtensionType::get_required_init_account_extensions(&[ExtensionType::TransferHook]);
    let account_len = ExtensionType::try_calculate_account_len::<PodAccount>(&extension_types)
        .expect("calculate transfer hook account len");
    let mut data = vec![0u8; account_len];
    let mut token_account =
        PodStateWithExtensionsMut::<PodAccount>::unpack_uninitialized(&mut data).unwrap();

    *token_account.base = PodAccount {
        mint: *mint,
        owner: *owner,
        amount: PodU64::from(amount),
        delegate: PodCOption::none(),
        state: AccountState::Initialized as u8,
        is_native: PodCOption::none(),
        delegated_amount: PodU64::from(0),
        close_authority: PodCOption::none(),
    };
    token_account.init_account_type().expect("init token account type");
    for extension_type in extension_types {
        token_account
            .init_account_extension_from_type(extension_type)
            .expect("init token account extension");
    }

    let transfer_hook_account = token_account
        .get_extension_mut::<TransferHookAccount>()
        .expect("read transfer hook account extension");
    transfer_hook_account.transferring = PodBool::from_bool(false);

    Account {
        lamports: 1_000_000,
        data,
        owner: token2022::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn insert_account(context: &TestContext, pubkey: Pubkey, account: Account) {
    context.account_store.borrow_mut().insert(pubkey, account);
}

pub fn insert_wallet(context: &TestContext, pubkey: Pubkey) {
    insert_account(context, pubkey, wallet_account());
}

pub fn insert_mint(context: &TestContext, mint: &Pubkey) {
    insert_account(context, *mint, transfer_hook_mint_account_with_delegate(9, None));
}

pub fn insert_token_account(
    context: &TestContext,
    token_account: Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
    amount: u64,
) {
    insert_account(context, token_account, transfer_hook_token_account(mint, owner, amount));
}

pub fn role_member_account(role: RoleType, account_key: &Pubkey, bump: u8) -> Account {
    let mut data = RoleMember::DISCRIMINATOR.to_vec();
    data.extend(
        borsh::to_vec(&RoleMember { role, account: to_program_pubkey(account_key), bump }).unwrap(),
    );
    Account {
        lamports: 1_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn insert_role_member(
    context: &TestContext,
    state_key: &Pubkey,
    role: RoleType,
    member: &Pubkey,
) -> Pubkey {
    let (role_member, bump) = role_member_pda(state_key, role, member);
    insert_account(context, role_member, role_member_account(role, member, bump));
    role_member
}

pub fn allowlist_entry_account(is_blacklisted: bool, is_whitelisted: bool) -> Account {
    let mut data = AllowlistEntry::DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&AllowlistEntry { is_blacklisted, is_whitelisted }).unwrap());
    // Compatibility with pre-migration test binaries/accounts that still carry the removed bump.
    data.push(0);
    Account {
        lamports: 1_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn insert_allowlist_entry(
    context: &TestContext,
    hook_state: &Pubkey,
    pubkey: &Pubkey,
    is_blacklisted: bool,
    is_whitelisted: bool,
) -> Pubkey {
    let (entry, _) = allowlist_entry_pda(hook_state, pubkey);
    insert_account(context, entry, allowlist_entry_account(is_blacklisted, is_whitelisted));
    entry
}

pub fn insert_blacklist_entry(
    context: &TestContext,
    hook_state: &Pubkey,
    pubkey: &Pubkey,
) -> Pubkey {
    insert_allowlist_entry(context, hook_state, pubkey, true, false)
}

pub fn insert_whitelist_entry(
    context: &TestContext,
    hook_state: &Pubkey,
    pubkey: &Pubkey,
) -> Pubkey {
    insert_allowlist_entry(context, hook_state, pubkey, false, true)
}

pub fn read_account<T: AccountDeserialize>(context: &TestContext, pubkey: &Pubkey) -> T {
    let store = context.account_store.borrow();
    let account = store.get(pubkey).expect("account missing from store");
    let mut data = account.data.as_slice();
    T::try_deserialize(&mut data).expect("failed to deserialize account")
}

pub fn read_raw_account(context: &TestContext, pubkey: &Pubkey) -> Account {
    context
        .account_store
        .borrow()
        .get(pubkey)
        .expect("account missing from store")
        .clone()
}

pub fn account_exists(context: &TestContext, pubkey: &Pubkey) -> bool {
    context.account_store.borrow().contains_key(pubkey)
}

pub fn init_transfer_hook_info_ix(payer: &Pubkey, transfer_hook_info: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(*transfer_hook_info, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
        ],
        data: ix::InitTransferHookInfo::DISCRIMINATOR.to_vec(),
    }
}

pub fn init_transfer_hook_ix(
    payer: &Pubkey,
    transfer_hook_authority: &Pubkey,
    mint: &Pubkey,
    extra_account_meta_list: &Pubkey,
    hook_state: &Pubkey,
    admin_role_member: &Pubkey,
    admin: &Pubkey,
    paused: bool,
    allowlist_mode: AllowlistMode,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*transfer_hook_authority, true),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new(*extra_account_meta_list, false),
            AccountMeta::new(*hook_state, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new(*admin_role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(
            ix::InitTransferHook::DISCRIMINATOR,
            InitTransferHookParams { admin: to_program_pubkey(admin), paused, allowlist_mode },
        ),
    }
}

pub fn init_default_fixture(
    context: &TestContext,
    allowlist_mode: AllowlistMode,
    paused: bool,
) -> (Pubkey, Pubkey, Pubkey, Pubkey, Pubkey) {
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let mint = mint_key();
    let (hook_state, _) = hook_state_pda(&mint);
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(&mint);
    let (admin_role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    insert_wallet(context, payer);
    insert_wallet(context, transfer_hook_authority);
    insert_wallet(context, admin);
    insert_mint(context, &mint);

    context.process_and_validate_instruction(
        &init_transfer_hook_ix(
            &payer,
            &transfer_hook_authority,
            &mint,
            &extra_account_meta_list,
            &hook_state,
            &admin_role_member,
            &admin,
            paused,
            allowlist_mode,
        ),
        &[mollusk_svm::result::Check::success()],
    );

    (payer, transfer_hook_authority, admin, mint, hook_state)
}

pub fn setup_live_transfer_fixture(
    context: &TestContext,
    allowlist_mode: AllowlistMode,
    paused: bool,
) -> LiveTransferFixture {
    setup_live_transfer_fixture_with_permanent_delegate(context, allowlist_mode, paused, None)
}

pub fn setup_live_transfer_fixture_with_permanent_delegate(
    context: &TestContext,
    allowlist_mode: AllowlistMode,
    paused: bool,
    permanent_delegate: Option<Pubkey>,
) -> LiveTransferFixture {
    let source_owner = transfer_owner_key();
    let recipient_owner = transfer_recipient_key();
    let source_token = source_token_account_key();
    let destination_token = destination_token_account_key();
    let payer = payer_key();
    let transfer_hook_authority = default_admin_key();
    let admin = default_admin_key();
    let mint = mint_key();
    let (hook_state, _) = hook_state_pda(&mint);

    let (extra_account_meta_list, _) = extra_account_meta_list_pda(&mint);
    let (admin_role_member, _) = role_member_pda(&hook_state, RoleType::DefaultAdmin, &admin);

    insert_wallet(context, payer);
    insert_wallet(context, transfer_hook_authority);
    insert_wallet(context, admin);
    insert_account(context, mint, transfer_hook_mint_account_with_delegate(9, permanent_delegate));
    context.process_and_validate_instruction(
        &init_transfer_hook_ix(
            &payer,
            &transfer_hook_authority,
            &mint,
            &extra_account_meta_list,
            &hook_state,
            &admin_role_member,
            &admin,
            paused,
            allowlist_mode,
        ),
        &[mollusk_svm::result::Check::success()],
    );

    insert_wallet(context, source_owner);
    insert_wallet(context, recipient_owner);
    insert_account(
        context,
        source_token,
        transfer_hook_token_account(&mint, &source_owner, 1_000_000),
    );
    insert_account(
        context,
        destination_token,
        transfer_hook_token_account(&mint, &recipient_owner, 0),
    );

    LiveTransferFixture {
        payer,
        transfer_hook_authority,
        admin,
        hook_state,
        mint,
        source_owner,
        source_token,
        destination_token,
    }
}

pub fn set_allowlist_mode_ix(
    authority: &Pubkey,
    hook_state: &Pubkey,
    role_member: &Pubkey,
    mode: AllowlistMode,
) -> Instruction {
    set_allowlist_mode_raw_ix(authority, hook_state, role_member, mode as u8)
}

pub fn set_allowlist_mode_raw_ix(
    authority: &Pubkey,
    hook_state: &Pubkey,
    role_member: &Pubkey,
    mode: u8,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*hook_state, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetAllowlistMode::DISCRIMINATOR, mode),
    }
}

pub fn set_blacklisted_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    hook_state: &Pubkey,
    allowlist_entry: &Pubkey,
    role_member: &Pubkey,
    pubkey: &Pubkey,
    is_enabled: bool,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new(*allowlist_entry, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(
            ix::SetBlacklisted::DISCRIMINATOR,
            SetAllowlistedParams { pubkey: to_program_pubkey(pubkey), is_enabled },
        ),
    }
}

pub fn set_whitelisted_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    hook_state: &Pubkey,
    allowlist_entry: &Pubkey,
    role_member: &Pubkey,
    pubkey: &Pubkey,
    is_enabled: bool,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new(*allowlist_entry, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(
            ix::SetWhitelisted::DISCRIMINATOR,
            SetAllowlistedParams { pubkey: to_program_pubkey(pubkey), is_enabled },
        ),
    }
}

pub fn is_blacklisted_ix(hook_state: &Pubkey, pubkey: &Pubkey, entry: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new_readonly(*entry, false),
        ],
        data: ix_data(ix::IsBlacklisted::DISCRIMINATOR, to_program_pubkey(pubkey)),
    }
}

pub fn is_whitelisted_ix(hook_state: &Pubkey, pubkey: &Pubkey, entry: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new_readonly(*entry, false),
        ],
        data: ix_data(ix::IsWhitelisted::DISCRIMINATOR, to_program_pubkey(pubkey)),
    }
}

pub fn is_allowlisted_ix(
    hook_state: &Pubkey,
    pubkey: &Pubkey,
    allowlist_entry: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new_readonly(*allowlist_entry, false),
        ],
        data: ix_data(ix::IsAllowlisted::DISCRIMINATOR, to_program_pubkey(pubkey)),
    }
}

pub fn token_ix(hook_state: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![AccountMeta::new_readonly(*hook_state, false)],
        data: ix::Token::DISCRIMINATOR.to_vec(),
    }
}

pub fn pause_ix(authority: &Pubkey, hook_state: &Pubkey, role_member: &Pubkey) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*hook_state, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::Pause::DISCRIMINATOR, ()),
    }
}

pub fn unpause_ix(authority: &Pubkey, hook_state: &Pubkey, role_member: &Pubkey) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*hook_state, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::Unpause::DISCRIMINATOR, ()),
    }
}

pub fn set_bypass_ix(
    payer: &Pubkey,
    authority: &Pubkey,
    hook_state: &Pubkey,
    bypass_entry: &Pubkey,
    role_member: &Pubkey,
    account: &Pubkey,
    is_enabled: bool,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new_readonly(*hook_state, false),
            AccountMeta::new(*bypass_entry, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(
            ix::SetBypass::DISCRIMINATOR,
            SetBypassParams { account: to_program_pubkey(account), is_enabled },
        ),
    }
}

pub fn transfer_checked_with_hook_ix(
    context: &TestContext,
    source: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    owner: &Pubkey,
    amount: u64,
    decimals: u8,
) -> Instruction {
    block_on(create_transfer_checked_instruction_with_extra_metas(
        &token2022::ID,
        source,
        mint,
        destination,
        owner,
        &[],
        amount,
        decimals,
        |address| {
            ready(Ok(context
                .account_store
                .borrow()
                .get(&address)
                .map(|account| account.data.clone())))
        },
    ))
    .expect("build token-2022 transfer instruction")
}

fn load_transaction_accounts(
    context: &TestContext,
    instructions: &[Instruction],
) -> Vec<(Pubkey, Account)> {
    let mut accounts = Vec::new();
    let mut seen = HashSet::new();
    let store = context.account_store.borrow();
    instructions.iter().for_each(|instruction| {
        instruction.accounts.iter().for_each(|AccountMeta { pubkey, .. }| {
            if seen.insert(*pubkey) {
                let account =
                    store.get_account(pubkey).unwrap_or_else(|| store.default_account(pubkey));
                accounts.push((*pubkey, account));
            }
        });
    });
    accounts
}

pub fn process_and_validate_transaction_instructions(
    context: &TestContext,
    instructions: &[Instruction],
    checks: &[mollusk_svm::result::Check],
) -> TransactionResult {
    let accounts = load_transaction_accounts(context, instructions);
    let result = context.mollusk.process_and_validate_transaction_instructions(
        instructions,
        &accounts,
        checks,
    );
    if result.program_result.is_ok() {
        let mut store = context.account_store.borrow_mut();
        for (pubkey, account) in &result.resulting_accounts {
            store.insert(*pubkey, account.clone());
        }
    }
    result
}

pub fn process_transaction_instructions(
    context: &TestContext,
    instructions: &[Instruction],
) -> TransactionResult {
    let accounts = load_transaction_accounts(context, instructions);
    let result = context.mollusk.process_transaction_instructions(instructions, &accounts);
    if result.program_result.is_ok() {
        let mut store = context.account_store.borrow_mut();
        for (pubkey, account) in &result.resulting_accounts {
            store.insert(*pubkey, account.clone());
        }
    }
    result
}

pub fn token_amount(context: &TestContext, pubkey: &Pubkey) -> u64 {
    let account = context.account_store.borrow().get(pubkey).cloned().expect("token missing");
    let state = PodStateWithExtensions::<PodAccount>::unpack(&account.data)
        .expect("unpack token account with extensions");
    u64::from(state.base.amount)
}

pub fn program_err(error: impl Into<u32>) -> ProgramError {
    ProgramError::Custom(error.into())
}

pub fn expect_program_failure(result: &mollusk_svm::result::InstructionResult, error: HookError) {
    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(program_err(error))
    );
}

pub fn expect_transaction_program_failure(
    result: &TransactionResult,
    instruction_index: usize,
    error: HookError,
) {
    assert_eq!(
        result.program_result,
        TransactionProgramResult::Failure(instruction_index, program_err(error))
    );
}
