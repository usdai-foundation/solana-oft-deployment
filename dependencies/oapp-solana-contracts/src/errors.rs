use anchor_lang::error_code;

/// Custom error codes for OApp operations
#[error_code]
pub enum OAppError {
    #[msg("Invalid address lookup table")]
    InvalidAddressLookupTable,
    #[msg("Invalid options")]
    InvalidOptions,
    #[msg("Invalid options length")]
    InvalidOptionsLength,
    #[msg("No peer set for the given EID")]
    NoPeer,
    #[msg("TLV update would change entry size")]
    TlvLengthMismatch,
}
