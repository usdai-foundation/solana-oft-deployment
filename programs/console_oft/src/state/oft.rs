use crate::{seeds::ESCROW_SEED, OFTError};
use anchor_lang::prelude::*;
use oapp::OAppState;
use oft::{types::OftType, OFT_SEED};
use rbac::DefaultAdmin;

pub const MAX_ALTS: usize = 10;

#[account]
#[derive(InitSpace, DefaultAdmin)]
pub struct OFTStore {
    // ---------------------------------------------------------------------------
    // Immutable (set once at initialization)
    // ---------------------------------------------------------------------------
    pub oft_type: OftType,
    pub ld2sd_rate: u64,
    pub shared_decimals: u8,
    pub token_mint: Pubkey,
    // Unique discriminator in OFTStore PDA seeds; the corresponding key must
    // sign `init_console_oft` to enforce path authorization (no one can create an
    // OFTStore at someone else's initializer-PDA without their key). Distinct
    // from `current_default_admin` so admin can be rotated without changing
    // the OFTStore address.
    pub initializer: Pubkey,
    pub token_program: Pubkey,
    pub bump: u8,
    // ---------------------------------------------------------------------------
    // Mutable (updated by instructions)
    // ---------------------------------------------------------------------------
    pub transfer_hook_program: Pubkey, // synced via sync_transfer_hook_program
    #[max_len(MAX_ALTS)]
    pub alts: Vec<Pubkey>,
    // embedded default admin management
    pub current_default_admin: Pubkey,
    pub pending_default_admin: Pubkey,
    // embedded default pause
    pub default_paused: bool,
    // embedded default fee
    pub default_fee_bps: u16,
    // embedded fee handler
    pub fee_deposit: Pubkey,
    // embedded rate limiter global config
    pub use_global_state: bool,
    pub is_globally_disabled: bool,
}

impl OFTStore {
    /// PDA seeds for the OFT token escrow (child PDA of the OFT store).
    /// Seed shape: [ESCROW_SEED, oft_store_key]
    pub fn token_escrow_seeds(oft_store_key: &Pubkey) -> &'static [&'static [u8]] {
        let key_bytes: &'static [u8; 32] = Box::leak(Box::new(oft_store_key.to_bytes()));
        Box::leak(Box::new([ESCROW_SEED, key_bytes.as_slice()]))
    }

    pub fn ld2sd(&self, amount_ld: u64) -> u64 {
        amount_ld / self.ld2sd_rate
    }

    pub fn sd2ld(&self, amount_sd: u64) -> Result<u64> {
        amount_sd
            .checked_mul(self.ld2sd_rate)
            .ok_or(OFTError::ArithmeticOverflow.into())
    }

    pub fn remove_dust(&self, amount_ld: u64) -> u64 {
        amount_ld - amount_ld % self.ld2sd_rate
    }
}

impl OAppState<3> for OFTStore {
    fn signer_seeds(&self) -> [&[u8]; 3] {
        [OFT_SEED, self.initializer.as_ref(), std::slice::from_ref(&self.bump)]
    }
}
