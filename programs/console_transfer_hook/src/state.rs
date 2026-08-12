//! Program state for Console Transfer Hook
//!
//! This module defines the account structures used by the Transfer Hook:
//! - [`TransferHookState`] — per-Token-2022-mint configuration (pause bit, allowlist mode, RBAC
//!   admin)
//! - [`AllowlistEntry`] — unified marker PDA carrying both blacklist and whitelist flags
//! - [`BypassEntry`] — marker PDA exempting a source token account from hook checks
//!
//! # EVM Parity Notes
//!
//! ## Enumeration Functions
//!
//! The EVM implementation provides these view functions that are NOT available on Solana:
//! - `blacklistedCount()` / `whitelistedCount()`
//! - `getBlacklist(offset, limit)` / `getWhitelist(offset, limit)`
//!
//! This is an architectural limitation: Solana PDAs cannot be enumerated on-chain.
//! Off-chain indexers should track blacklist/whitelist/bypass membership by listening
//! to `BlacklistUpdated`, `WhitelistUpdated`, and `BypassUpdated` events.
//!
//! ## Checking Membership
//!
//! To check if a pubkey is blacklisted/whitelisted:
//! - EVM: `isBlacklisted(user)` / `isWhitelisted(user)`
//! - Solana: Read the unified PDA `["allowlist", hook_state, pubkey]`. Its `is_blacklisted` /
//!   `is_whitelisted` flags carry the state; a missing PDA means neither flag is set.

use anchor_lang::prelude::*;
use rbac::DefaultAdmin;

use crate::AllowlistMode;

// =============================================================================
// PDA Seed Constants
// =============================================================================

pub const TRANSFER_HOOK_SEED: &[u8] = b"transfer-hook";
pub const BYPASS_SEED: &[u8] = b"bypass";
pub const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";
pub const ALLOWLIST_SEED: &[u8] = b"allowlist";

/// The transfer hook state account, holding global configuration and RBAC data for a specific
/// Token-2022 mint.
///
/// Seeds: `[TRANSFER_HOOK_SEED, mint]`
#[account]
#[derive(Default, InitSpace, DefaultAdmin)]
pub struct TransferHookState {
    /// The Token-2022 mint this hook state configures.
    pub mint: Pubkey,
    /// Pause bit for the transfer hook. When true, transfers are blocked unless
    /// a source bypass entry or PermanentDelegate fund recovery path applies.
    pub paused: bool,
    /// Allowlist mode: Open (ignore list entries), Blacklist (block if `is_blacklisted`), or
    /// Whitelist (allow if `is_whitelisted`).
    pub allowlist_mode: AllowlistMode,
    /// Holder of the DefaultAdmin role.
    pub current_default_admin: Pubkey,
    /// Pending half of the 2-step DefaultAdmin handoff.
    pub pending_default_admin: Pubkey,
    pub bump: u8,
}

/// Unified allowlist marker PDA. Stores both list bits for the subject pubkey;
/// the PDA exists if at least one flag is true.
///
/// Seeds: `[ALLOWLIST_SEED, hook_state, pubkey]`
#[account]
#[derive(Default, InitSpace)]
pub struct AllowlistEntry {
    /// When true, the subject pubkey is blacklisted in blacklist mode.
    pub is_blacklisted: bool,
    /// When true, the subject pubkey is whitelisted in whitelist mode.
    pub is_whitelisted: bool,
}

impl AllowlistEntry {
    /// Decode flags from an entry PDA that may not exist; returns
    /// `(false, false)` when the PDA has no data.
    pub fn read_flags(account: &UncheckedAccount) -> Result<(bool, bool)> {
        if account.data_is_empty() {
            return Ok((false, false));
        }
        let data = account.try_borrow_data()?;
        let entry = AllowlistEntry::try_deserialize(&mut &data[..])?;
        Ok((entry.is_blacklisted, entry.is_whitelisted))
    }

    /// Close the PDA and refund its lamports to `sol_destination` once both
    /// flags become false — preserves the invariant that the PDA exists iff
    /// at least one list flag is set.
    pub fn close_if_empty<'info>(
        account: &mut Account<'info, Self>,
        sol_destination: AccountInfo<'info>,
    ) -> Result<()> {
        if !account.is_blacklisted && !account.is_whitelisted {
            account.close(sol_destination)?;
        }
        Ok(())
    }
}

/// Bypass entry
///
/// Seeds: `[BYPASS_SEED, hook_state, pubkey]`
///
/// PDA existence is the bit: when this PDA exists with data, the seeded
/// `pubkey` bypasses all transfer hook checks (pause and allowlist) when it
/// is the source of a transfer for this hook state's mint.
///
/// `initialized` lets the `set_bypass` toggle enforce idempotency on top of
/// `init_if_needed`. The hook itself still reads existence via
/// `data_is_empty()` — disabling closes the PDA, so the existence bit and
/// `initialized` stay aligned.
///
/// Primary use case: OFT escrow accounts for cross-chain receives.
/// Matches EVM where `mint()` bypasses pause and allowlist modifiers.
#[account]
#[derive(Default, InitSpace)]
pub struct BypassEntry {
    pub initialized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_state_size() {
        // 32 + 1 + 1 + 32 + 32 + 1  = 99
        assert_eq!(TransferHookState::INIT_SPACE, 99);
    }

    #[test]
    fn test_hook_state_default_values() {
        let config = TransferHookState::default();
        assert_eq!(config.mint, Pubkey::default());
        assert!(!config.paused);
        assert_eq!(config.allowlist_mode, AllowlistMode::Open);
        assert_eq!(config.current_default_admin, Pubkey::default());
        assert_eq!(config.pending_default_admin, Pubkey::default());
        assert_eq!(config.bump, 0);
    }

    #[test]
    fn test_allowlist_entry_size() {
        // 1 (is_blacklisted) + 1 (is_whitelisted) = 2
        assert_eq!(AllowlistEntry::INIT_SPACE, 2);
    }

    #[test]
    fn test_bypass_entry_size() {
        // 1 (initialized) = 1
        assert_eq!(BypassEntry::INIT_SPACE, 1);
    }
}
