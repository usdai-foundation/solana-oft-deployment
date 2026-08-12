//! Console-OFT program event catalog.
//!
//! Every `#[event]` the program emits lives here so off-chain indexers and SDKs
//! have a single place to look up the wire schema.
//!
//! Each event carries an `oft_store` pubkey: a single OFT program manages many
//! independent OFTStore instances (one per OFT deployment, mirroring how the
//! SPL Token program manages many `Mint` accounts), so events carry the store
//! pubkey to let off-chain consumers attribute each event to the right
//! instance.

use anchor_lang::prelude::*;

use crate::state::{RateLimitConfig, RateLimitState};

// ================================ Pause (IPauseByID) ================================

/// Emitted when the default pause status is set.
#[event]
pub struct DefaultPauseSet {
    pub oft_store: Pubkey,
    pub paused: bool,
}

/// Emitted when the pause status is set for a specific destination EID.
#[event]
pub struct PauseSet {
    pub oft_store: Pubkey,
    pub id: u128,
    /// Per-ID pause override. `None` = override removed (fallback to default).
    pub paused: Option<bool>,
}

// ================================ Fee Config (IFeeConfig) ================================

/// Emitted when the default fee basis points (BPS) are set.
#[event]
pub struct DefaultFeeBpsSet {
    pub oft_store: Pubkey,
    pub fee_bps: u16,
}

/// Emitted when the fee basis points (BPS) are set for a specific destination EID.
#[event]
pub struct FeeBpsSet {
    pub oft_store: Pubkey,
    pub id: u128,
    /// Per-ID fee override in basis points. `None` = override removed (fallback to default).
    pub fee_bps: Option<u16>,
}

// ================================ Fee Handler (IFeeHandler) ================================

/// Emitted when the fee deposit is updated.
#[event]
pub struct FeeDepositSet {
    pub oft_store: Pubkey,
    pub fee_deposit: Pubkey,
}

// ================================ Rate Limiter (IRateLimiter) ================================

/// Emitted when a rate limit configuration is updated.
///
/// Note: `id` is 0 for root/default config updates, matching EVM's DEFAULT_ID.
#[event]
pub struct RateLimitConfigUpdated {
    pub oft_store: Pubkey,
    pub id: u128,
    pub config: RateLimitConfig,
}

/// Emitted when the global rate limiter configuration is updated.
#[event]
pub struct RateLimitGlobalConfigUpdated {
    pub oft_store: Pubkey,
    pub use_global_state: bool,
    pub is_globally_disabled: bool,
}

/// Emitted when a rate limit address exemption is updated.
#[event]
pub struct RateLimitAddressExemptionUpdated {
    pub oft_store: Pubkey,
    pub user: Pubkey,
    pub is_exempt: bool,
}

/// Emitted when a rate limit state is manually updated.
#[event]
pub struct RateLimitStateUpdated {
    pub oft_store: Pubkey,
    pub id: u128,
    pub state: RateLimitState,
}
