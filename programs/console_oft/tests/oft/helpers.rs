#![allow(dead_code)]

use anchor_lang::{
    prelude::borsh, AccountDeserialize, AnchorDeserialize, AnchorSerialize, Discriminator,
};
use console_oft::{
    instruction as ix,
    instructions::{
        fee_config::set_fee_bps::SetFeeBpsParams,
        pause::set_paused::SetPausedParams,
        rate_limiter::{
            set_rate_limit_address_exemption::SetRateLimitAddressExemptionParams,
            set_rate_limit_config::SetRateLimitConfigParams,
            set_rate_limit_global_config::SetRateLimitGlobalConfigParams,
            set_rate_limit_state::SetRateLimitStateParams,
        },
    },
    seeds::{FEE_SEED, PAUSE_SEED, RATE_LIMIT_EXEMPTION_SEED, RATE_LIMIT_SEED},
    state::{
        FeeConfig, OFTStore, PauseConfig, RateLimit, RateLimitConfig, RateLimitState, RoleType,
        MAX_ALTS,
    },
    OAppPeer, RoleMember,
};
use mollusk_svm::{Mollusk, MolluskContext};
use mollusk_svm_programs_token::token2022;
use mollusk_svm_result::types::ProgramResult;
use oapp::{OAPP_INFO_SEED, PEER_SEED};
use oft::types::OftType;
use rbac::{RbacError, ROLE_MEMBER_SEED};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_program_option::COption;
use solana_program_pack::Pack;
use solana_pubkey::Pubkey;
use spl_token_2022::state::{Account as TokenAccount, AccountState, Mint as TokenMint};
use std::{collections::HashMap, sync::Once};

pub const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([0u8; 32]);
pub const DST_EID: u32 = 2;
pub const TOKEN_DECIMALS: u8 = 6;

static QUIET_MOLLUSK_LOGS: Once = Once::new();

pub type TestContext = MolluskContext<HashMap<Pubkey, Account>>;

pub struct OftFixture {
    pub context: TestContext,
    pub payer: Pubkey,
    pub admin: Pubkey,
    pub delegate: Pubkey,
    pub oft_store: Pubkey,
    pub token_mint: Pubkey,
    pub initializer: Pubkey,
    pub fee_deposit: Pubkey,
    pub default_admin_role_member: Pubkey,
    pub fee_manager_role_member: Pubkey,
    pub pauser_role_member: Pubkey,
    pub unpauser_role_member: Pubkey,
    pub rate_limiter_role_member: Pubkey,
    pub default_rate_limit: Pubkey,
    pub peer: Pubkey,
}

#[derive(Clone)]
pub struct QuoteOftIxParams {
    pub dst_eid: u32,
    pub to: [u8; 32],
    pub amount_ld: u64,
    pub min_amount_ld: u64,
    pub extra_options: Vec<u8>,
    pub compose_msg: Vec<u8>,
    pub oft_cmd: Vec<u8>,
    pub pay_in_lz_token: bool,
}

pub fn program_id() -> Pubkey {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(console_oft::ID.as_ref());
    Pubkey::new_from_array(bytes)
}

pub fn mollusk() -> Mollusk {
    quiet_mollusk_logs();
    let binary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join("Cargo.lock").exists())
        .expect("Cargo.lock not found")
        .join("target/deploy/console_oft");
    let mut mollusk = Mollusk::new(&program_id(), binary.to_str().unwrap());
    token2022::add_program(&mut mollusk);
    mollusk
}

pub fn current_unix_timestamp() -> u64 {
    mollusk().sysvars.clock.unix_timestamp.try_into().unwrap()
}

pub fn mollusk_with_clock(unix_timestamp: i64) -> Mollusk {
    let mut m = mollusk();
    m.sysvars.clock.unix_timestamp = unix_timestamp;
    m
}

pub fn context() -> TestContext {
    mollusk().with_context(HashMap::new())
}

pub fn context_with_clock(unix_timestamp: i64) -> TestContext {
    mollusk_with_clock(unix_timestamp).with_context(HashMap::new())
}

fn quiet_mollusk_logs() {
    QUIET_MOLLUSK_LOGS.call_once(|| {
        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var("RUST_LOG", "off");
        }
    });
}

pub fn payer_key() -> Pubkey {
    Pubkey::new_from_array([0xAA; 32])
}

pub fn admin_key() -> Pubkey {
    Pubkey::new_from_array([0xAB; 32])
}

pub fn delegate_key() -> Pubkey {
    Pubkey::new_from_array([0xAC; 32])
}

pub fn oft_store_key() -> Pubkey {
    Pubkey::new_from_array([0xB1; 32])
}

pub fn token_mint_key() -> Pubkey {
    Pubkey::new_from_array([0xB2; 32])
}

pub fn initializer_key() -> Pubkey {
    Pubkey::new_from_array([0xB3; 32])
}

pub fn fee_deposit_key() -> Pubkey {
    Pubkey::new_from_array([0xB4; 32])
}

pub fn to_program_pubkey(key: &Pubkey) -> anchor_lang::prelude::Pubkey {
    anchor_lang::prelude::Pubkey::new_from_array(*key.as_array())
}

/// Assert that an `__event_cpi` inner-instruction's data decodes to `expected`.
///
/// Layout: `[__event_cpi discriminator (8)][event discriminator (8)][borsh fields]`.
pub fn assert_event<T: AnchorSerialize + Discriminator>(data: &[u8], expected: T) {
    assert_eq!(&data[8..16], T::DISCRIMINATOR, "event discriminator mismatch");
    let actual_bytes = &data[16..];
    let expected_bytes = borsh::to_vec(&expected).expect("serialize expected");
    assert_eq!(actual_bytes, expected_bytes.as_slice(), "event payload mismatch");
}

pub fn event_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"__event_authority"], &program_id())
}

pub fn oapp_info_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[OAPP_INFO_SEED], &program_id())
}

pub fn role_member_pda(state_key: &Pubkey, role: RoleType, member: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ROLE_MEMBER_SEED, state_key.as_ref(), &[role as u8], member.as_ref()],
        &program_id(),
    )
}

pub fn peer_pda(oft_store: &Pubkey, eid: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[PEER_SEED, oft_store.as_ref(), &eid.to_be_bytes()],
        &program_id(),
    )
}

pub fn fee_config_pda(oft_store: &Pubkey, id: u128) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[FEE_SEED, oft_store.as_ref(), &id.to_be_bytes()], &program_id())
}

pub fn pause_config_pda(oft_store: &Pubkey, id: u128) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[PAUSE_SEED, oft_store.as_ref(), &id.to_be_bytes()],
        &program_id(),
    )
}

pub fn rate_limit_pda(oft_store: &Pubkey, id: u128) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[RATE_LIMIT_SEED, oft_store.as_ref(), &id.to_be_bytes()],
        &program_id(),
    )
}

pub fn rate_limit_address_exemption_pda(oft_store: &Pubkey, user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[RATE_LIMIT_EXEMPTION_SEED, oft_store.as_ref(), user.as_ref()],
        &program_id(),
    )
}

pub fn wallet_account() -> Account {
    Account::new(10_000_000, 0, &SYSTEM_PROGRAM)
}

pub fn token_mint_account(decimals: u8) -> Account {
    let mint = TokenMint {
        mint_authority: COption::Some(admin_key()),
        supply: 1_000_000,
        decimals,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    let mut data = vec![0u8; TokenMint::LEN];
    TokenMint::pack(mint, &mut data).expect("pack mint");
    Account {
        lamports: 10_000_000,
        data,
        owner: token2022::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn token_account_account(mint: &Pubkey, owner: &Pubkey) -> Account {
    let token_account = TokenAccount {
        mint: *mint,
        owner: *owner,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    let mut data = vec![0u8; TokenAccount::LEN];
    TokenAccount::pack(token_account, &mut data).expect("pack token account");
    Account {
        lamports: 10_000_000,
        data,
        owner: token2022::ID,
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn oft_store_account(
    token_mint: &Pubkey,
    initializer: &Pubkey,
    fee_deposit: &Pubkey,
    default_admin: &Pubkey,
) -> Account {
    let mut data = OFTStore::DISCRIMINATOR.to_vec();
    data.extend(
        borsh::to_vec(&OFTStore {
            oft_type: OftType::LockUnlock,
            ld2sd_rate: 1,
            shared_decimals: TOKEN_DECIMALS,
            token_mint: to_program_pubkey(token_mint),
            initializer: to_program_pubkey(initializer),
            token_program: to_program_pubkey(&token2022::ID),
            bump: 255,
            transfer_hook_program: anchor_lang::prelude::Pubkey::default(),
            alts: Vec::with_capacity(MAX_ALTS),
            current_default_admin: to_program_pubkey(default_admin),
            pending_default_admin: anchor_lang::prelude::Pubkey::default(),
            default_paused: false,
            default_fee_bps: 0,
            fee_deposit: to_program_pubkey(fee_deposit),
            use_global_state: false,
            is_globally_disabled: false,
        })
        .unwrap(),
    );

    Account {
        lamports: 10_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn role_member_account(role: RoleType, account_key: &Pubkey, bump: u8) -> Account {
    let mut data = RoleMember::DISCRIMINATOR.to_vec();
    data.extend(
        borsh::to_vec(&RoleMember { role, account: to_program_pubkey(account_key), bump }).unwrap(),
    );
    Account {
        lamports: 10_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn peer_account(address: [u8; 32], bump: u8) -> Account {
    let mut data = OAppPeer::DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&OAppPeer { address, bump }).unwrap());
    Account {
        lamports: 10_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn rate_limit_account(config: RateLimitConfig, state: RateLimitState, _bump: u8) -> Account {
    let mut data = RateLimit::DISCRIMINATOR.to_vec();
    data.extend(borsh::to_vec(&RateLimit { config, state }).unwrap());
    Account {
        lamports: 10_000_000,
        data,
        owner: program_id(),
        executable: false,
        rent_epoch: u64::MAX,
    }
}

pub fn insert_wallet(context: &TestContext, key: Pubkey) {
    context.account_store.borrow_mut().insert(key, wallet_account());
}

pub fn insert_role_member(
    context: &TestContext,
    oft_store: &Pubkey,
    role: RoleType,
    member: &Pubkey,
) -> Pubkey {
    let (role_member, bump) = role_member_pda(oft_store, role, member);
    context
        .account_store
        .borrow_mut()
        .insert(role_member, role_member_account(role, member, bump));
    role_member
}

pub fn insert_peer(context: &TestContext, oft_store: &Pubkey, eid: u32) -> Pubkey {
    let (peer, bump) = peer_pda(oft_store, eid);
    context.account_store.borrow_mut().insert(peer, peer_account([0x44; 32], bump));
    peer
}

pub fn insert_rate_limit(
    context: &TestContext,
    oft_store: &Pubkey,
    id: u128,
    config: RateLimitConfig,
    state: RateLimitState,
) -> Pubkey {
    let (rate_limit, bump) = rate_limit_pda(oft_store, id);
    context
        .account_store
        .borrow_mut()
        .insert(rate_limit, rate_limit_account(config, state, bump));
    rate_limit
}

pub fn fixture() -> OftFixture {
    build_fixture_from_context(context())
}

pub fn fixture_with_clock(unix_timestamp: i64) -> OftFixture {
    build_fixture_from_context(context_with_clock(unix_timestamp))
}

fn build_fixture_from_context(context: TestContext) -> OftFixture {
    let payer = payer_key();
    let admin = admin_key();
    let delegate = delegate_key();
    let oft_store = oft_store_key();
    let token_mint = token_mint_key();
    let initializer = initializer_key();
    let fee_deposit = fee_deposit_key();

    insert_wallet(&context, payer);
    insert_wallet(&context, admin);
    insert_wallet(&context, delegate);
    context
        .account_store
        .borrow_mut()
        .insert(token_mint, token_mint_account(TOKEN_DECIMALS));
    context
        .account_store
        .borrow_mut()
        .insert(fee_deposit, token_account_account(&token_mint, &admin));
    context
        .account_store
        .borrow_mut()
        .insert(oft_store, oft_store_account(&token_mint, &initializer, &fee_deposit, &admin));

    let default_admin_role_member =
        insert_role_member(&context, &oft_store, RoleType::DefaultAdmin, &admin);
    let fee_manager_role_member =
        insert_role_member(&context, &oft_store, RoleType::FeeConfigManager, &admin);
    let pauser_role_member = insert_role_member(&context, &oft_store, RoleType::Pauser, &admin);
    let unpauser_role_member = insert_role_member(&context, &oft_store, RoleType::Unpauser, &admin);
    let rate_limiter_role_member =
        insert_role_member(&context, &oft_store, RoleType::RateLimiterManager, &admin);
    let default_rate_limit = insert_rate_limit(
        &context,
        &oft_store,
        0,
        RateLimitConfig::default(),
        RateLimitState::default(),
    );
    let peer = insert_peer(&context, &oft_store, DST_EID);

    OftFixture {
        context,
        payer,
        admin,
        delegate,
        oft_store,
        token_mint,
        initializer,
        fee_deposit,
        default_admin_role_member,
        fee_manager_role_member,
        pauser_role_member,
        unpauser_role_member,
        rate_limiter_role_member,
        default_rate_limit,
        peer,
    }
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
        .cloned()
        .expect("account missing from store")
}

pub fn read_account_data<T: AnchorDeserialize + Discriminator>(
    context: &TestContext,
    pubkey: &Pubkey,
) -> T {
    let account = read_raw_account(context, pubkey);
    assert_eq!(&account.data[..8], T::DISCRIMINATOR, "unexpected discriminator");
    T::try_from_slice(&account.data[8..]).expect("failed to deserialize account data")
}

pub fn read_fee_config(context: &TestContext, pubkey: &Pubkey) -> FeeConfig {
    read_account_data(context, pubkey)
}

pub fn read_pause_config(context: &TestContext, pubkey: &Pubkey) -> PauseConfig {
    read_account_data(context, pubkey)
}

pub fn read_rate_limit(context: &TestContext, pubkey: &Pubkey) -> RateLimit {
    read_account_data(context, pubkey)
}

pub fn program_err(error: impl Into<u32>) -> ProgramError {
    ProgramError::Custom(error.into())
}

pub fn rbac_unauthorized() -> ProgramError {
    program_err(RbacError::Unauthorized)
}

pub fn program_failure(error: impl Into<u32>) -> ProgramResult {
    ProgramResult::Failure(program_err(error))
}

pub fn rbac_failure() -> ProgramResult {
    ProgramResult::Failure(rbac_unauthorized())
}

pub fn ix_data<T: AnchorSerialize>(discriminator: &[u8], params: T) -> Vec<u8> {
    let mut data = discriminator.to_vec();
    data.extend(borsh::to_vec(&params).unwrap());
    data
}

pub fn quote_oft_ix_data(discriminator: &[u8], params: &QuoteOftIxParams) -> Vec<u8> {
    let mut data = discriminator.to_vec();
    data.extend_from_slice(&params.dst_eid.to_le_bytes());
    data.extend_from_slice(&params.to);
    data.extend_from_slice(&params.amount_ld.to_le_bytes());
    data.extend_from_slice(&params.min_amount_ld.to_le_bytes());

    for bytes in [&params.extra_options, &params.compose_msg, &params.oft_cmd] {
        data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(bytes);
    }

    data.push(u8::from(params.pay_in_lz_token));
    data
}

pub fn set_default_fee_bps_ix(
    authority: &Pubkey,
    oft_store: &Pubkey,
    role_member: &Pubkey,
    fee_bps: u16,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetDefaultFeeBps::DISCRIMINATOR, fee_bps),
    }
}

pub fn init_console_oft_info_ix(payer: &Pubkey, oapp_info: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(*oapp_info, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
        ],
        data: ix::InitConsoleOftInfo::DISCRIMINATOR.to_vec(),
    }
}

pub fn set_fee_bps_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    oft_store: &Pubkey,
    fee_config: &Pubkey,
    role_member: &Pubkey,
    params: SetFeeBpsParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new(*fee_config, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetFeeBps::DISCRIMINATOR, params),
    }
}

pub fn set_fee_deposit_ix(
    authority: &Pubkey,
    oft_store: &Pubkey,
    fee_deposit: &Pubkey,
    role_member: &Pubkey,
    params: Pubkey,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new_readonly(*fee_deposit, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetFeeDeposit::DISCRIMINATOR, to_program_pubkey(&params)),
    }
}

pub fn set_default_paused_ix(
    authority: &Pubkey,
    oft_store: &Pubkey,
    role_member: &Pubkey,
    paused: bool,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetDefaultPaused::DISCRIMINATOR, paused),
    }
}

pub fn set_paused_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    oft_store: &Pubkey,
    pause_config: &Pubkey,
    role_member: &Pubkey,
    params: SetPausedParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new(*pause_config, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetPaused::DISCRIMINATOR, params),
    }
}

pub fn set_rate_limit_global_config_ix(
    authority: &Pubkey,
    oft_store: &Pubkey,
    role_member: &Pubkey,
    params: SetRateLimitGlobalConfigParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetRateLimitGlobalConfig::DISCRIMINATOR, params),
    }
}

/// Build a `set_rate_limit_config` instruction.
pub fn set_rate_limit_config_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    oft_store: &Pubkey,
    rate_limit: &Pubkey,
    role_member: &Pubkey,
    params: SetRateLimitConfigParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new(*rate_limit, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetRateLimitConfig::DISCRIMINATOR, params),
    }
}

/// Build a `set_rate_limit_state` instruction.
pub fn set_rate_limit_state_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    oft_store: &Pubkey,
    rate_limit: &Pubkey,
    role_member: &Pubkey,
    params: SetRateLimitStateParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new(*rate_limit, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetRateLimitState::DISCRIMINATOR, params),
    }
}

pub fn set_rate_limit_address_exemption_ix(
    authority: &Pubkey,
    payer: &Pubkey,
    oft_store: &Pubkey,
    exemption: &Pubkey,
    role_member: &Pubkey,
    params: SetRateLimitAddressExemptionParams,
) -> Instruction {
    let (event_authority, _) = event_authority_pda();
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new(*exemption, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
            AccountMeta::new_readonly(*role_member, false),
            AccountMeta::new_readonly(event_authority, false),
            AccountMeta::new_readonly(program_id(), false),
        ],
        data: ix_data(ix::SetRateLimitAddressExemption::DISCRIMINATOR, params),
    }
}

/// Build a `checkpoint_rate_limit` instruction.
pub fn checkpoint_rate_limit_ix(
    authority: &Pubkey,
    oft_store: &Pubkey,
    default_rate_limit: &Pubkey,
    rate_limit: &Pubkey,
    role_member: &Pubkey,
    id: u128,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*oft_store, false),
            AccountMeta::new(*default_rate_limit, false),
            AccountMeta::new(*rate_limit, false),
            AccountMeta::new_readonly(*role_member, false),
        ],
        data: ix_data(ix::CheckpointRateLimit::DISCRIMINATOR, id),
    }
}

pub fn rate_limits_ix(oft_store: &Pubkey, rate_limiter: &Pubkey, id: u128) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new_readonly(*rate_limiter, false),
        ],
        data: ix_data(ix::RateLimits::DISCRIMINATOR, id),
    }
}

pub fn get_rate_limit_usages_ix(
    oft_store: &Pubkey,
    default_rate_limit: &Pubkey,
    rate_limiter: &Pubkey,
    id: u128,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new_readonly(*default_rate_limit, false),
            AccountMeta::new_readonly(*rate_limiter, false),
        ],
        data: ix_data(ix::GetRateLimitUsages::DISCRIMINATOR, id),
    }
}

pub fn is_rate_limit_address_exempt_ix(
    oft_store: &Pubkey,
    exemption: &Pubkey,
    user: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new_readonly(*exemption, false),
        ],
        data: ix_data(ix::IsRateLimitAddressExempt::DISCRIMINATOR, to_program_pubkey(user)),
    }
}

pub fn quote_oft_ix(
    oft_store: &Pubkey,
    pause_config: &Pubkey,
    fee_config: &Pubkey,
    default_rate_limit: &Pubkey,
    rate_limit: &Pubkey,
    params: QuoteOftIxParams,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(*oft_store, false),
            AccountMeta::new_readonly(*pause_config, false),
            AccountMeta::new_readonly(*fee_config, false),
            AccountMeta::new_readonly(*default_rate_limit, false),
            AccountMeta::new_readonly(*rate_limit, false),
        ],
        data: quote_oft_ix_data(ix::QuoteOft::DISCRIMINATOR, &params),
    }
}

pub fn decode_return<T: AnchorDeserialize>(data: &[u8]) -> T {
    T::try_from_slice(data).expect("decode return data")
}

pub fn quote_params(amount_ld: u64, min_amount_ld: u64) -> QuoteOftIxParams {
    QuoteOftIxParams {
        dst_eid: DST_EID,
        to: [0x55; 32],
        amount_ld,
        min_amount_ld,
        extra_options: Vec::new(),
        compose_msg: Vec::new(),
        oft_cmd: Vec::new(),
        pay_in_lz_token: false,
    }
}
