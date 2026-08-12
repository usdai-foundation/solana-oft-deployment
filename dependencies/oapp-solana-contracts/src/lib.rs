//! LayerZero OApp SDK for Solana.
//!
//! Provides the core building blocks for Solana programs that integrate with the
//! LayerZero V2 cross-chain messaging protocol as an Omnichain Application (OApp).
//!
//! # What's included
//!
//! - **PDA seeds** – canonical constants for deriving all protocol-related accounts (peers,
//!   enforced options, nonces, payload hashes, etc.).
//! - **Endpoint CPI helpers** – thin wrappers around cross-program invocations to the LayerZero
//!   Endpoint program (send, quote, compose, etc.).
//! - **Account macros** – re-exported from [`oapp_macros`] to generate boilerplate Anchor account
//!   structs for standard OApp instructions.
//! - **Types & traits** – shared parameter / return structs, the [`OAppState`] trait for PDA signer
//!   seeds, and messaging option builders.

pub mod common;
pub mod endpoint_cpi;
pub mod events;
pub mod lz_compose_types_v2;
pub mod lz_receive_types_v2;
pub mod oapp_info;
pub mod options;
pub mod types;
pub mod utils;

mod errors;
use anchor_lang::prelude::*;
// `idls/endpoint.json` is vendored here because `declare_program!` reads the
// IDL at compile time, and downstream consumers of this crate cannot rely on
// a workspace-relative path to the Endpoint program's own build output.
declare_program!(endpoint);

pub use errors::*;

// Re-export procedural macros so consumers can `use oapp::oapp`
pub use oapp_macros::*;

// OApp state & config
pub const OAPP_SEED: &[u8] = b"OApp";
pub const OAPP_INFO_SEED: &[u8] = b"OAppInfo";
pub const PEER_SEED: &[u8] = b"Peer";
pub const ENFORCED_OPTIONS_SEED: &[u8] = b"EnforcedOptions";

// Messaging
pub const LZ_COMPOSE_TYPES_SEED: &[u8] = b"LzComposeTypes";
pub const LZ_RECEIVE_TYPES_SEED: &[u8] = b"LzReceiveTypes";
pub const NONCE_SEED: &[u8] = b"Nonce";
pub const PENDING_NONCE_SEED: &[u8] = b"PendingNonce";
pub const PAYLOAD_HASH_SEED: &[u8] = b"PayloadHash";
pub const COMPOSED_MESSAGE_HASH_SEED: &[u8] = b"ComposedMessageHash";

// Endpoint & library config
pub const ENDPOINT_SEED: &[u8] = b"Endpoint";
pub const MESSAGE_LIB_SEED: &[u8] = b"MessageLib";
pub const SEND_LIBRARY_CONFIG_SEED: &[u8] = b"SendLibraryConfig";
pub const RECEIVE_LIBRARY_CONFIG_SEED: &[u8] = b"ReceiveLibraryConfig";

// Event
pub const EVENT_SEED: &[u8] = b"__event_authority";

// PDA seed helpers below use `Box::leak` to produce the `&'static [&'static [u8]]`
// shape required by Anchor's `#[account(seeds = ...)]` attribute. Each call leaks
// ~64 bytes. **SVM-only**: harmless on-chain because the BPF VM allocator is torn
// down at the end of every transaction, so leaks never accumulate. Do not call
// these from long-running off-chain Rust (native test runners, indexers, CLIs) —
// derive PDAs with stack-local arrays in those contexts instead.

/// PDA seeds for an OApp's `lz_receive_types_accounts` PDA.
/// Seed shape: [LZ_RECEIVE_TYPES_SEED, oapp_key]
pub fn lz_receive_types_seeds(oapp_key: &Pubkey) -> &'static [&'static [u8]] {
    let key_bytes: &'static [u8; 32] = Box::leak(Box::new(oapp_key.to_bytes()));
    Box::leak(Box::new([LZ_RECEIVE_TYPES_SEED, key_bytes.as_slice()]))
}

/// Trait for OApp state types.
///
/// Defines the minimal interface for PDA signer seeds used in CPI invocations.
/// `N` is the number of seed segments (e.g. 3 for `[PREFIX, key, bump]`).
pub trait OAppState<const N: usize> {
    fn signer_seeds(&self) -> [&[u8]; N];
}
