//! Program events for Console Transfer Hook
//!
//! Events are emitted using Anchor's `emit_cpi!` macro and can be indexed
//! by off-chain services for monitoring and analytics.
//!
//! Event naming follows EVM ERC20Plus conventions where applicable.
//! Solana-specific events cover Token-2022 hook-state initialization and bypass entry toggles.
//!
//! `mint` is the Token-2022 mint whose hook configuration changed.
//!
//! Note: RBAC events (RoleGranted, RoleRevoked) are defined in the `rbac` crate.

use crate::AllowlistMode;
use anchor_lang::prelude::*;

/// Emitted when TransferHookState is initialized for a mint
#[event]
pub struct Initialized {
    pub mint: Pubkey,
    pub transfer_hook_authority: Pubkey,
    pub allowlist_mode: AllowlistMode,
    pub paused: bool,
}

/// Emitted when the allowlist mode is changed.
#[event]
pub struct AllowlistModeUpdated {
    pub mint: Pubkey,
    pub new_mode: AllowlistMode,
}

/// Emitted when a pubkey's blacklist status changes.
///
/// EVM equivalent: `BlacklistUpdated(address subject, bool isBlacklisted)`
/// (Solana adds `mint` so indexers can scope events to the mint.)
#[event]
pub struct BlacklistUpdated {
    pub mint: Pubkey,
    pub pubkey: Pubkey,
    pub is_blacklisted: bool,
}

/// Emitted when a pubkey's whitelist status changes.
///
/// EVM equivalent: `WhitelistUpdated(address subject, bool isWhitelisted)`
/// (Solana adds `mint` so indexers can scope events to the mint.)
#[event]
pub struct WhitelistUpdated {
    pub mint: Pubkey,
    pub pubkey: Pubkey,
    pub is_whitelisted: bool,
}

/// Emitted when a per-mint pause config changes.
///
/// EVM equivalent: `PauseSet(bool paused)`
/// (Solana adds `mint` so indexers can scope events to the mint.)
#[event]
pub struct PauseSet {
    pub mint: Pubkey,
    pub paused: bool,
}

/// Emitted when a `(mint, account)` bypass entry is toggled.
///
/// Parallels `BlacklistUpdated`/`WhitelistUpdated`: a single event covers
/// both "added" and "removed" transitions via `is_enabled`.
#[event]
pub struct BypassUpdated {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub is_enabled: bool,
}
