//! Read-only allowlist views — EVM parity:
//! - `is_blacklisted` → `IAllowlist.isBlacklisted(address)`
//! - `is_whitelisted` → `IAllowlist.isWhitelisted(address)`
//! - `is_allowlisted` → `IAllowlist.isAllowlisted(address)`

use crate::state::{AllowlistEntry, TransferHookState, ALLOWLIST_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(pubkey: Pubkey)]
pub struct AllowlistView<'info> {
    pub hook_state: Account<'info, TransferHookState>,

    /// CHECK: Unified allowlist marker — may not exist when the subject is on
    /// neither list.
    #[account(
        seeds = [ALLOWLIST_SEED, hook_state.key().as_ref(), pubkey.as_ref()],
        bump
    )]
    pub allowlist_entry: UncheckedAccount<'info>,
}

impl AllowlistView<'_> {
    pub fn is_blacklisted(ctx: &Context<Self>, _pubkey: &Pubkey) -> Result<bool> {
        let (blacklisted, _) = AllowlistEntry::read_flags(&ctx.accounts.allowlist_entry)?;
        Ok(blacklisted)
    }

    pub fn is_whitelisted(ctx: &Context<Self>, _pubkey: &Pubkey) -> Result<bool> {
        let (_, whitelisted) = AllowlistEntry::read_flags(&ctx.accounts.allowlist_entry)?;
        Ok(whitelisted)
    }

    pub fn is_allowlisted(ctx: &Context<Self>, _pubkey: &Pubkey) -> Result<bool> {
        let (blacklisted, whitelisted) = AllowlistEntry::read_flags(&ctx.accounts.allowlist_entry)?;
        Ok(ctx.accounts.hook_state.allowlist_mode.allows(blacklisted, whitelisted))
    }
}
