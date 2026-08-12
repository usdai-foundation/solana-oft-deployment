//! Error codes for Console Transfer Hook

use anchor_lang::prelude::error_code;

#[error_code]
pub enum HookError {
    // =========================================================================
    // Transfer Hook Errors (returned during transfer execution)
    // =========================================================================
    /// Mint pause is active and no bypass path applies.
    #[msg("Transfers are paused")]
    Paused,

    /// Source token account is blocked by the allowlist
    #[msg("Source token account is blocked")]
    SourceBlocked,

    /// Destination token account is blocked by the allowlist
    #[msg("Destination token account is blocked")]
    DestinationBlocked,

    /// Authority (spender) is blocked by the allowlist
    #[msg("Authority is blocked")]
    AuthorityBlocked,

    /// PermanentDelegate can only transfer from blocked accounts (fund recovery).
    #[msg("Cannot recover funds from an allowlisted source")]
    CannotRecoverFromAllowlisted,

    // =========================================================================
    // Admin Instruction Errors
    // =========================================================================
    /// Allowlist mode is already set to this value
    #[msg("Mode already set to the same value")]
    ModeAlreadySet,

    /// Mint account data could not be unpacked as a Token-2022 mint.
    #[msg("Invalid mint account")]
    InvalidMint,

    /// Signer is not the token mint's transfer-hook authority (also raised
    /// when the mint's TransferHook extension has no authority set).
    #[msg("Invalid transfer hook authority")]
    InvalidTransferHookAuthority,

    /// Pause state unchanged (already paused/unpaused)
    #[msg("Pause state is already set to this value")]
    PauseStateIdempotent,

    /// Token mint does not have the TransferHook extension enabled
    #[msg("Missing transfer hook extension")]
    MissingTransferHookExtension,

    /// Token mint's TransferHook extension points at a different program
    /// (or has no program id set).
    #[msg("Invalid transfer hook program")]
    InvalidTransferHookProgram,

    /// Allowlist entry state unchanged (already enabled/disabled)
    #[msg("Allowlist state is already set to this value")]
    AllowlistStateIdempotent,

    /// Bypass entry state unchanged (already enabled/disabled)
    #[msg("Bypass state is already set to this value")]
    BypassStateIdempotent,
}
