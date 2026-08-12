use anchor_lang::prelude::error_code;

#[error_code]
pub enum OFTError {
    // ============================== Core ==============================
    /// remaining_accounts is shorter than what the instruction needs to slice
    #[msg("Insufficient remaining accounts")]
    InsufficientRemainingAccounts,
    #[msg("Mint decimals less than shared decimals")]
    InvalidDecimals,
    #[msg("Invalid ExtraAccountMetaList")]
    InvalidExtraAccountMetaList,
    #[msg("Invalid OFT message")]
    InvalidMessage,
    #[msg("Invalid mint authority")]
    InvalidMintAuthority,
    #[msg("Invalid sender")]
    InvalidSender,
    #[msg("Invalid clock timestamp")]
    InvalidTimestamp,
    #[msg("Invalid token destination")]
    InvalidTokenDest,
    #[msg("Invalid transfer hook program")]
    InvalidTransferHookProgram,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Slippage exceeded")]
    SlippageExceeded,
    #[msg("Too many ALTs")]
    TooManyAlts,

    // ============================== Fee ===============================
    #[msg("Fee bps exceeds max")]
    InvalidFee,
    /// EVM alignment: IFeeHandler.InvalidFeeDeposit
    #[msg("Invalid fee deposit")]
    InvalidFeeDeposit,

    // ============================== Pause =============================
    #[msg("Pause state unchanged")]
    PauseStateIdempotent,
    #[msg("OFT is paused")]
    Paused,

    // =========================== Rate Limiter =========================
    /// EVM alignment: IRateLimiter.ExemptionStateIdempotent
    #[msg("Exemption state unchanged")]
    ExemptionStateIdempotent,
    /// EVM alignment: IRateLimiter.LastUpdatedInFuture
    #[msg("Last updated in future")]
    LastUpdatedInFuture,
    #[msg("Rate limit exceeded")]
    RateLimitExceeded,
    #[msg("Rate limit not initialized")]
    RateLimitNotInitialized,
}
