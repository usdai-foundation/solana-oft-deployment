//! Read-only views on `TransferHookState`: pause bit and allowlist mode.

use crate::{state::TransferHookState, AllowlistMode};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct TransferHookView<'info> {
    pub hook_state: Account<'info, TransferHookState>,
}

impl TransferHookView<'_> {
    pub fn allowlist_mode(ctx: &Context<Self>) -> Result<AllowlistMode> {
        Ok(ctx.accounts.hook_state.allowlist_mode)
    }

    pub fn is_paused(ctx: &Context<Self>) -> Result<bool> {
        Ok(ctx.accounts.hook_state.paused)
    }

    pub fn token(ctx: &Context<Self>) -> Result<Pubkey> {
        Ok(ctx.accounts.hook_state.mint)
    }
}
