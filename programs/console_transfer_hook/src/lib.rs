//! Console Transfer Hook
//!
//! A Token-2022 Transfer Hook that implements compliance controls:
//! - Pause/unpause transfers
//! - Allowlist modes (open, blacklist, or whitelist)
//! - Per-account bypass list for OFT escrows
//! - PermanentDelegate recovery bypass for fund recovery from blocked source token accounts
//!
//! # ERC20Plus-equivalent Architecture
//!
//! Each Token-2022 mint has a `TransferHookState` PDA that owns its pause, allowlist,
//! bypass, and RBAC state. Init must be signed by the Token-2022 transfer-hook
//! authority; ongoing administration is handled by the TransferHookState's 2-step
//! DefaultAdmin.
//!
//! Every PDA below is per-mint; the "Extra key" column shows what scopes it
//! beyond the mint.
//!
//! | Account                | Extra key | Purpose                              |
//! |------------------------|-----------|--------------------------------------|
//! | `TransferHookState`    | —         | Pause state, allowlist mode, RBAC    |
//! | `ExtraAccountMetaList` | —         | Token-2022 hook account resolution   |
//! | `BypassEntry`          | account   | Bypass checks for specific accounts  |
//! | `AllowlistEntry`       | pubkey    | Blacklist or whitelist marker        |
//!
//! # Pause
//!
//! `TransferHookState.paused` is the pause bit, matching ERC20Plus.
//!
//! Check order: fund recovery bypass → source bypass → pause → allowlist
//!
//! # Allowlist Scope
//!
//! Source and destination checks are keyed by token-account pubkey. Delegate
//! checks are keyed by signer pubkey.
//!
//! This matches native SPL/Token-2022 freeze at transfer time: freeze is
//! token-account scoped, not wallet scoped. Deterministic ATA pubkeys can be
//! marked before account creation; mint and burn are handled by OFT logic
//! because Token-2022 has no mint/burn hook.
//!
//! # OFT Bypass Setup Flow
//!
//! 1. `init_transfer_hook` - Creates TransferHookState, ExtraAccountMetaList, and DefaultAdmin
//! 2. `grant_role(BypassManager, ...)` - Grant BypassManager role
//! 3. `set_bypass` - Add OFT escrow to the bypass set
//!
//! # Events
//!
//! This program emits Anchor events for hook configuration state changes.
//! Each event carries `mint` so indexers can scope it to the mint. RBAC
//! state-change events are emitted by the `rbac` crate.
//! - `Initialized` - TransferHookState initialization
//! - `AllowlistModeUpdated` - when mode changes
//! - `BlacklistUpdated` - blacklist status changes
//! - `WhitelistUpdated` - whitelist status changes
//! - `PauseSet` - pause state changes
//! - `BypassUpdated` - bypass entry add/remove
//!
//! # Version
//!
//! `TransferHookInfo` publishes program-level schema and IDL versions for SDK tooling.

use anchor_lang::prelude::*;
use solana_helper::program_id_from_env;
use spl_discriminator::SplDiscriminate;
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;
pub mod transfer_hook_info;
pub mod types;

pub use events::*;
pub use instructions::*;
pub use state::*;
pub use transfer_hook_info::{
    IdlVersion, TransferHookInfo, TRANSFER_HOOK_INFO_IDL_VERSION,
    TRANSFER_HOOK_INFO_SCHEMA_VERSION, TRANSFER_HOOK_INFO_SEED,
};
pub use types::*;

declare_id!(Pubkey::new_from_array(program_id_from_env!(
    "HOOK_ID",
    "Hook111111111111111111111111111111111111111"
)));

#[rbac::rbac(state = TransferHookState, role_type = RoleType)]
#[program]
pub mod console_transfer_hook {
    /// Initializes the program-level metadata PDA used by SDK tooling.
    pub fn init_transfer_hook_info(mut ctx: Context<InitTransferHookInfo>) -> Result<()> {
        InitTransferHookInfo::apply(&mut ctx)
    }

    /// Register a mint with the transfer hook
    ///
    /// Creates the ExtraAccountMetaList PDA that tells Token-2022 what extra
    /// accounts to pass to the transfer hook. Must be called for each mint
    /// before transfers can work.
    ///
    /// Requires the mint's Token-2022 transfer-hook authority.
    pub fn init_transfer_hook(
        mut ctx: Context<InitTransferHook>,
        params: InitTransferHookParams,
    ) -> Result<()> {
        InitTransferHook::apply(&mut ctx, &params)
    }

    /// Returns the Token-2022 mint configured for this transfer hook state.
    pub fn token(ctx: Context<TransferHookView>) -> Result<Pubkey> {
        TransferHookView::token(&ctx)
    }

    /// Transfer hook - called by Token-2022 on every transfer
    ///
    /// This is the core compliance check. It verifies:
    /// 1. PermanentDelegate fund recovery rules
    /// 2. Source bypass entry
    /// 3. Mint pause state
    /// 4. Source, destination, and signer allowlist rules
    ///
    /// Uses `#[instruction(discriminator = ...)]` to override Anchor's discriminator
    /// with the SPL Transfer Hook Interface discriminator, allowing Token-2022 to
    /// call this function directly without a fallback handler.
    #[instruction(discriminator = ExecuteInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn transfer_hook(ctx: Context<TransferHook>, params: TransferHookParams) -> Result<()> {
        TransferHook::apply(&ctx, &params)
    }

    // =========================================================================
    // Bypass
    // =========================================================================

    /// Toggle a `(mint, account)` bypass entry on/off.
    ///
    /// When enabled, the seeded `account` (a token account, not a wallet)
    /// bypasses all transfer-hook compliance checks (pause + allowlist) while
    /// it is the source of a transfer for the seeded mint.
    ///
    /// Requires the `BypassManager` role.
    pub fn set_bypass(mut ctx: Context<SetBypass>, params: SetBypassParams) -> Result<()> {
        SetBypass::apply(&mut ctx, &params)
    }

    // =========================================================================
    // Pause
    // =========================================================================

    /// Pause transfers for this mint.
    ///
    /// Requires the `Pauser` role.
    pub fn pause(mut ctx: Context<Pause>) -> Result<()> {
        Pause::apply(&mut ctx)
    }

    /// Unpause transfers for this mint.
    ///
    /// Requires the `Unpauser` role.
    pub fn unpause(mut ctx: Context<Unpause>) -> Result<()> {
        Unpause::apply(&mut ctx)
    }

    /// Returns whether a specific mint is paused.
    pub fn is_paused(ctx: Context<TransferHookView>) -> Result<bool> {
        TransferHookView::is_paused(&ctx)
    }

    // =========================================================================
    // Allowlist
    // =========================================================================

    /// Set allowlist mode (Open, Blacklist, or Whitelist)
    pub fn set_allowlist_mode(
        mut ctx: Context<SetAllowlistMode>,
        mode: types::AllowlistMode,
    ) -> Result<()> {
        SetAllowlistMode::apply(&mut ctx, mode)
    }

    /// Set a pubkey's blacklist status
    ///
    /// Creates the unified `AllowlistEntry` if absent; closes it when both flags clear.
    pub fn set_blacklisted(
        mut ctx: Context<SetBlacklisted>,
        params: SetAllowlistedParams,
    ) -> Result<()> {
        SetBlacklisted::apply(&mut ctx, &params)
    }

    /// Set a pubkey's whitelist status
    ///
    /// Creates the unified `AllowlistEntry` if absent; closes it when both flags clear.
    pub fn set_whitelisted(
        mut ctx: Context<SetWhitelisted>,
        params: SetAllowlistedParams,
    ) -> Result<()> {
        SetWhitelisted::apply(&mut ctx, &params)
    }

    /// Returns the current allowlist mode.
    pub fn allowlist_mode(ctx: Context<TransferHookView>) -> Result<AllowlistMode> {
        TransferHookView::allowlist_mode(&ctx)
    }

    /// Returns whether a pubkey is blacklisted.
    pub fn is_blacklisted(ctx: Context<AllowlistView>, pubkey: Pubkey) -> Result<bool> {
        AllowlistView::is_blacklisted(&ctx, &pubkey)
    }

    /// Returns whether a pubkey is whitelisted.
    pub fn is_whitelisted(ctx: Context<AllowlistView>, pubkey: Pubkey) -> Result<bool> {
        AllowlistView::is_whitelisted(&ctx, &pubkey)
    }

    /// Returns whether a pubkey is allowed under the current allowlist mode.
    pub fn is_allowlisted(ctx: Context<AllowlistView>, pubkey: Pubkey) -> Result<bool> {
        AllowlistView::is_allowlisted(&ctx, &pubkey)
    }
}
