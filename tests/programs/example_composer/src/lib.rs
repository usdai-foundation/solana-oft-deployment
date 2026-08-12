//! # Example Composer Program
//!
//! A minimal example demonstrating how to implement a LayerZero composer
//! using the `#[composer]` macro from `oapp-macros`.
//!
//! **This is for integration-testing purposes only.**

pub mod instructions;

use anchor_lang::prelude::*;
use instructions::*;
use oapp::{
    composer,
    lz_compose_types_v2::{LzComposeTypesV2Accounts, LzComposeTypesV2Result},
    types::LzComposeParams,
};

declare_id!("Comp111111111111111111111111111111111111111");

pub const COMPOSER_SEED: &[u8] = b"Composer";

/// Global state for the example composer program.
///
/// Stores a counter for testing/verification purposes.
#[account]
#[derive(InitSpace)]
pub struct ComposerStore {
    /// Counter incremented each time lz_compose is called (for testing)
    pub compose_count: u64,
    pub bump: u8,
}

impl ComposerStore {
    pub fn signer_seeds(&self) -> [&[u8]; 2] {
        [COMPOSER_SEED, std::slice::from_ref(&self.bump)]
    }
}

#[composer]
pub mod example_composer {
    pub fn init(mut ctx: Context<Init>) -> Result<()> {
        Init::apply(&mut ctx)
    }

    #[composer_instruction]
    pub fn lz_compose_types_info(
        ctx: &Context<LzComposeTypesInfo>,
        params: &LzComposeParams,
    ) -> Result<(u8, LzComposeTypesV2Accounts)> {
        LzComposeTypesInfo::apply(ctx, params)
    }

    #[composer_instruction]
    pub fn lz_compose_types_v2(
        ctx: &Context<LzComposeTypesV2>,
        params: &LzComposeParams,
    ) -> Result<LzComposeTypesV2Result> {
        LzComposeTypesV2::apply(ctx, params)
    }

    #[composer_instruction]
    pub fn lz_compose(ctx: &mut Context<LzCompose>, params: &LzComposeParams) -> Result<()> {
        LzCompose::apply(ctx, params)
    }
}
