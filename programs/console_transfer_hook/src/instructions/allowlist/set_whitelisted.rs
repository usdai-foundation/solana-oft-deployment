//! Set Whitelisted instruction
//!
//! Toggles the `is_whitelisted` flag on the unified `AllowlistEntry` PDA,
//! opening the PDA on first true flag and closing it once both flags clear.
//! EVM parity: AllowlistRBACUpgradeable.setWhitelisted(SetAllowlistParam[])
//!
//! Requires the Whitelister role.

use crate::{
    errors::HookError,
    state::{AllowlistEntry, TransferHookState, ALLOWLIST_SEED},
    RoleMember, RoleType, SetAllowlistedParams, WhitelistUpdated,
};
use anchor_lang::prelude::*;

#[rbac::only_role(role = RoleType::Whitelister, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
#[instruction(params: SetAllowlistedParams)]
pub struct SetWhitelisted<'info> {
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub hook_state: Account<'info, TransferHookState>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + AllowlistEntry::INIT_SPACE,
        seeds = [ALLOWLIST_SEED, hook_state.key().as_ref(), params.pubkey.as_ref()],
        bump,
        constraint = allowlist_entry.is_whitelisted != params.is_enabled
            @ HookError::AllowlistStateIdempotent,
    )]
    pub allowlist_entry: Account<'info, AllowlistEntry>,

    pub system_program: Program<'info, System>,
}

impl SetWhitelisted<'_> {
    pub fn apply(ctx: &mut Context<Self>, params: &SetAllowlistedParams) -> Result<()> {
        let entry = &mut ctx.accounts.allowlist_entry;
        entry.is_whitelisted = params.is_enabled;
        AllowlistEntry::close_if_empty(entry, ctx.accounts.payer.to_account_info())?;

        emit_cpi!(WhitelistUpdated {
            mint: ctx.accounts.hook_state.mint,
            pubkey: params.pubkey,
            is_whitelisted: params.is_enabled,
        });

        Ok(())
    }
}
