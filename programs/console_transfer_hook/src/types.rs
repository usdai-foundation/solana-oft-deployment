//! Public enums shared by instructions, state, and SDK clients:
//! RBAC role types and allowlist operating modes.

use anchor_lang::prelude::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use rbac::RoleType;

/// Role types for the Console Transfer Hook program.
///
/// Aligned with OpenZeppelin AccessControl and EVM ERC20Plus.
/// All roles are administered by DefaultAdmin (first variant).
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Default,
    RoleType,
    TryFromPrimitive,
    IntoPrimitive,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum RoleType {
    /// Root admin role — administers itself and all other roles.
    /// Equivalent to OpenZeppelin's DEFAULT_ADMIN_ROLE.
    #[default]
    DefaultAdmin = 0,
    /// Can pause transfers
    Pauser = 1,
    /// Can unpause transfers
    Unpauser = 2,
    /// Can add/remove blacklist entries
    Blacklister = 3,
    /// Can add/remove whitelist entries
    Whitelister = 4,
    /// Can add/remove bypass entries (e.g. OFT escrows)
    BypassManager = 5,
}

// =============================================================================
// AllowlistMode
// =============================================================================

/// Allowlist operating modes.
///
/// - `Open`: allowlist entries are ignored
/// - `Blacklist`: listed pubkeys are blocked
/// - `Whitelist`: only listed pubkeys are allowed
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Default, Debug, InitSpace,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum AllowlistMode {
    #[default]
    Open = 0,
    Blacklist = 1,
    Whitelist = 2,
}

impl AllowlistMode {
    /// Decide whether a subject is allowed under the current mode given its
    /// blacklist/whitelist flags.
    #[inline]
    pub fn allows(self, blacklisted: bool, whitelisted: bool) -> bool {
        match self {
            AllowlistMode::Open => true,
            AllowlistMode::Blacklist => !blacklisted,
            AllowlistMode::Whitelist => whitelisted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_full_matrix() {
        let cases = [
            (AllowlistMode::Open, false, false, true),
            (AllowlistMode::Open, true, false, true),
            (AllowlistMode::Open, false, true, true),
            (AllowlistMode::Open, true, true, true),
            (AllowlistMode::Blacklist, false, false, true),
            (AllowlistMode::Blacklist, true, false, false),
            (AllowlistMode::Blacklist, false, true, true),
            (AllowlistMode::Blacklist, true, true, false),
            (AllowlistMode::Whitelist, false, false, false),
            (AllowlistMode::Whitelist, true, false, false),
            (AllowlistMode::Whitelist, false, true, true),
            (AllowlistMode::Whitelist, true, true, true),
        ];
        for (mode, bl, wl, expected) in cases {
            assert_eq!(mode.allows(bl, wl), expected);
        }
    }
}
