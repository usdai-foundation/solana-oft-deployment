//! CPI helpers for invoking the LayerZero Endpoint program.
//!
//! Each function in this module is a thin wrapper that:
//! 1. Validates the caller-supplied authority key matches the expected account in the accounts
//!    slice.
//! 2. Validates that `accounts[0]` is the Endpoint program itself (checked against
//!    `endpoint_program`) and that enough accounts were supplied.
//! 3. Invokes the corresponding Endpoint instruction, signing with the OApp's PDA seeds.
//!
//! **Important:** The `accounts` slice must follow the field declaration order of the
//! corresponding `cpi::accounts::*` struct, with `endpoint_program` prepended at index 0.
//! See each wrapper's "Accounts layout" section for the exact positional order.
//!
//! These wrappers are the primary interface OApp programs use to interact with the
//! LayerZero Endpoint for cross-chain messaging operations.
//!
//! The `endpoint` IDL (`idls/endpoint.json`) was obtained with:
//!
//! ```text
//! anchor idl fetch 76y77prsiCMvXMjuoZ5VRrhG5qYBrUMYTE5WgHqgjEn6 --provider.cluster mainnet
//! ```

use crate::endpoint::{
    self,
    cpi::accounts::{Clear, ClearCompose, Quote, RegisterOapp, Send, SendCompose, SetDelegate},
    types::{
        ClearComposeParams, ClearParams, MessagingFee, MessagingReceipt, QuoteParams,
        RegisterOAppParams, SendComposeParams, SendParams, SetDelegateParams,
    },
};
use anchor_lang::{
    error,
    prelude::{AccountInfo, CpiContext, ErrorCode, Key, Pubkey, Result},
    require, require_keys_eq, ToAccountInfos, ToAccountMetas,
};

pub const REGISTER_OAPP_MIN_ACCOUNTS_LEN: usize = 7;
pub const SET_DELEGATE_MIN_ACCOUNTS_LEN: usize = 5;
pub const SEND_MIN_ACCOUNTS_LEN: usize = 10;
pub const QUOTE_MIN_ACCOUNTS_LEN: usize = 7;
pub const CLEAR_MIN_ACCOUNTS_LEN: usize = 8;
pub const SEND_COMPOSE_MIN_ACCOUNTS_LEN: usize = 7;
pub const CLEAR_COMPOSE_MIN_ACCOUNTS_LEN: usize = 5;

fn validate(accounts: &[AccountInfo], endpoint_program: Pubkey, min_len: usize) -> Result<()> {
    require!(accounts.len() >= min_len, ErrorCode::AccountNotEnoughKeys);
    require_keys_eq!(accounts[0].key(), endpoint_program, ErrorCode::InvalidProgramId);
    Ok(())
}

fn make_cpi_ctx<'info, T>(
    accounts: &[AccountInfo<'info>],
    cpi_accounts: T,
    min_len: usize,
) -> CpiContext<'info, 'info, 'info, 'info, T>
where
    T: ToAccountMetas + ToAccountInfos<'info>,
{
    let mut ctx = CpiContext::new(accounts[0].key(), cpi_accounts);
    if accounts.len() > min_len {
        ctx = ctx.with_remaining_accounts(accounts[min_len..].to_vec());
    }
    ctx
}

/// Registers the OApp with the LayerZero Endpoint.
///
/// Must be called once during program initialization to establish the OApp's identity
/// with the Endpoint. After registration, the Endpoint recognizes this program as a
/// valid sender/receiver of cross-chain messages.
///
/// # Accounts layout
/// The `accounts` slice must follow the `RegisterOapp` struct field order:
///
/// 0. `endpoint_program`
/// 1. `payer`
/// 2. `oapp`
/// 3. `oapp_registry`
/// 4. `system_program`
/// 5. `event_authority`
/// 6. `program`
/// 7. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[2]` must match the provided `oapp` key (the OApp PDA).
pub fn register_oapp(
    endpoint_program: Pubkey,
    oapp: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: RegisterOAppParams,
) -> Result<()> {
    validate(accounts, endpoint_program, REGISTER_OAPP_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[2].key(), oapp, ErrorCode::ConstraintAddress);

    let cpi_accounts = RegisterOapp {
        payer: accounts[1].clone(),
        oapp: accounts[2].clone(),
        oapp_registry: accounts[3].clone(),
        system_program: accounts[4].clone(),
        event_authority: accounts[5].clone(),
        program: accounts[6].clone(),
    };
    endpoint::cpi::register_oapp(
        make_cpi_ctx(accounts, cpi_accounts, REGISTER_OAPP_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )
}

/// Sets a delegate address authorized to configure the OApp's Endpoint settings.
///
/// The delegate can manage messaging libraries, executors, and other Endpoint-level
/// configuration on behalf of the OApp admin.
///
/// # Accounts layout
/// The `accounts` slice must follow the `SetDelegate` struct field order:
///
/// 0. `endpoint_program`
/// 1. `oapp`
/// 2. `oapp_registry`
/// 3. `event_authority`
/// 4. `program`
/// 5. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[1]` must match the provided `oapp` key.
pub fn set_delegate(
    endpoint_program: Pubkey,
    oapp: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: SetDelegateParams,
) -> Result<()> {
    validate(accounts, endpoint_program, SET_DELEGATE_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[1].key(), oapp, ErrorCode::ConstraintAddress);

    let cpi_accounts = SetDelegate {
        oapp: accounts[1].clone(),
        oapp_registry: accounts[2].clone(),
        event_authority: accounts[3].clone(),
        program: accounts[4].clone(),
    };
    endpoint::cpi::set_delegate(
        make_cpi_ctx(accounts, cpi_accounts, SET_DELEGATE_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )
}

/// Sends a cross-chain message through the LayerZero Endpoint.
///
/// This is the core outbound messaging function. The Endpoint assigns a nonce,
/// emits a packet event for DVNs to verify, and charges the messaging fee.
///
/// # Accounts layout
/// The `accounts` slice must follow the `Send` struct field order:
///
/// 0. `endpoint_program`
/// 1. `sender`
/// 2. `send_library_program`
/// 3. `send_library_config`
/// 4. `default_send_library_config`
/// 5. `send_library_info`
/// 6. `endpoint`
/// 7. `nonce`
/// 8. `event_authority`
/// 9. `program`
/// 10. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[1]` must match the provided `sender` key.
/// - The `sender` key must appear exactly once in the accounts slice to prevent duplicate-account
///   exploits that could manipulate fee deduction.
///
/// # Returns
/// A [`MessagingReceipt`] containing the assigned nonce and fee breakdown.
pub fn send(
    endpoint_program: Pubkey,
    sender: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: SendParams,
) -> Result<MessagingReceipt> {
    validate(accounts, endpoint_program, SEND_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[1].key(), sender, ErrorCode::ConstraintAddress);
    require!(
        accounts.iter().filter(|acc| acc.key == &sender).count() <= 1,
        ErrorCode::ConstraintAddress
    );

    let cpi_accounts = Send {
        sender: accounts[1].clone(),
        send_library_program: accounts[2].clone(),
        send_library_config: accounts[3].clone(),
        default_send_library_config: accounts[4].clone(),
        send_library_info: accounts[5].clone(),
        endpoint: accounts[6].clone(),
        nonce: accounts[7].clone(),
        event_authority: accounts[8].clone(),
        program: accounts[9].clone(),
    };
    let rtn = endpoint::cpi::send(
        make_cpi_ctx(accounts, cpi_accounts, SEND_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )?;
    Ok(rtn.get())
}

/// Queries the Endpoint for the fee required to send a cross-chain message.
///
/// This is a read-only CPI — no PDA signer seeds are needed. Call this before
/// [`send`] to determine the [`MessagingFee`] the caller must pay.
///
/// # Accounts layout
/// The `accounts` slice must follow the `Quote` struct field order:
///
/// 0. `endpoint_program`
/// 1. `send_library_program`
/// 2. `send_library_config`
/// 3. `default_send_library_config`
/// 4. `send_library_info`
/// 5. `endpoint`
/// 6. `nonce`
/// 7. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
pub fn quote(
    endpoint_program: Pubkey,
    accounts: &[AccountInfo],
    params: QuoteParams,
) -> Result<MessagingFee> {
    validate(accounts, endpoint_program, QUOTE_MIN_ACCOUNTS_LEN)?;

    let cpi_accounts = Quote {
        send_library_program: accounts[1].clone(),
        send_library_config: accounts[2].clone(),
        default_send_library_config: accounts[3].clone(),
        send_library_info: accounts[4].clone(),
        endpoint: accounts[5].clone(),
        nonce: accounts[6].clone(),
    };
    let result =
        endpoint::cpi::quote(make_cpi_ctx(accounts, cpi_accounts, QUOTE_MIN_ACCOUNTS_LEN), params)?;
    Ok(result.get())
}

/// Marks an inbound message as received by clearing it from the Endpoint.
///
/// Called during `lz_receive` processing to acknowledge that the OApp has consumed the
/// message payload for a given (srcEid, sender, nonce) tuple. This advances the
/// inbound nonce and allows subsequent messages to be delivered.
///
/// # Accounts layout
/// The `accounts` slice must follow the `Clear` struct field order:
///
/// 0. `endpoint_program`
/// 1. `signer`
/// 2. `oapp_registry`
/// 3. `nonce`
/// 4. `payload_hash`
/// 5. `endpoint`
/// 6. `event_authority`
/// 7. `program`
/// 8. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[1]` is the `signer` — pinned to `receiver`. The OApp PDA path signs via `seeds`; the
///   delegate path requires the delegate to already be a signer. The Endpoint accepts either the
///   OApp PDA (`params.receiver`) or the registered delegate as `signer`.
///
/// # Returns
/// The payload hash (`[u8; 32]`) of the cleared message.
pub fn clear(
    endpoint_program: Pubkey,
    receiver: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: ClearParams,
) -> Result<[u8; 32]> {
    validate(accounts, endpoint_program, CLEAR_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[1].key(), receiver, ErrorCode::ConstraintAddress);

    let cpi_accounts = Clear {
        signer: accounts[1].clone(),
        oapp_registry: accounts[2].clone(),
        nonce: accounts[3].clone(),
        payload_hash: accounts[4].clone(),
        endpoint: accounts[5].clone(),
        event_authority: accounts[6].clone(),
        program: accounts[7].clone(),
    };
    let result = endpoint::cpi::clear(
        make_cpi_ctx(accounts, cpi_accounts, CLEAR_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )?;
    Ok(result.get())
}

/// Sends a composed message to another program on the same chain.
///
/// Composition allows an OApp to trigger follow-up logic in a separate program after
/// receiving a cross-chain message. The Endpoint stores the composed message hash,
/// and the target program's `lz_compose` handler is invoked to process it.
///
/// # Accounts layout
/// The `accounts` slice must follow the `SendCompose` struct field order:
///
/// 0. `endpoint_program`
/// 1. `from`
/// 2. `payer`
/// 3. `compose_message`
/// 4. `system_program`
/// 5. `event_authority`
/// 6. `program`
/// 7. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[1]` must match the provided `from` key (the sending OApp PDA).
pub fn send_compose(
    endpoint_program: Pubkey,
    from: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: SendComposeParams,
) -> Result<()> {
    validate(accounts, endpoint_program, SEND_COMPOSE_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[1].key(), from, ErrorCode::ConstraintAddress);

    let cpi_accounts = SendCompose {
        from: accounts[1].clone(),
        payer: accounts[2].clone(),
        compose_message: accounts[3].clone(),
        system_program: accounts[4].clone(),
        event_authority: accounts[5].clone(),
        program: accounts[6].clone(),
    };
    endpoint::cpi::send_compose(
        make_cpi_ctx(accounts, cpi_accounts, SEND_COMPOSE_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )
}

/// Clears (acknowledges) a composed message after processing.
///
/// Called by the target program's `lz_compose` handler after it has consumed the
/// composed message. This marks the composition as complete in the Endpoint.
///
/// # Accounts layout
/// The `accounts` slice must follow the `ClearCompose` struct field order:
///
/// 0. `endpoint_program`
/// 1. `to`
/// 2. `compose_message`
/// 3. `event_authority`
/// 4. `program`
/// 5. remaining accounts (variable length)
///
/// # Accounts validation
/// - `accounts[0]` must equal the caller-supplied `endpoint_program` parameter.
/// - `accounts[1]` must match the provided `to` key (the receiving program's PDA).
pub fn clear_compose(
    endpoint_program: Pubkey,
    to: Pubkey,
    accounts: &[AccountInfo],
    seeds: &[&[u8]],
    params: ClearComposeParams,
) -> Result<()> {
    validate(accounts, endpoint_program, CLEAR_COMPOSE_MIN_ACCOUNTS_LEN)?;
    require_keys_eq!(accounts[1].key(), to, ErrorCode::ConstraintAddress);

    let cpi_accounts = ClearCompose {
        to: accounts[1].clone(),
        compose_message: accounts[2].clone(),
        event_authority: accounts[3].clone(),
        program: accounts[4].clone(),
    };
    endpoint::cpi::clear_compose(
        make_cpi_ctx(accounts, cpi_accounts, CLEAR_COMPOSE_MIN_ACCOUNTS_LEN).with_signer(&[seeds]),
        params,
    )
}
