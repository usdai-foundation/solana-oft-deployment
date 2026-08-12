//! Set Blacklisted instruction
//!
//! Toggles the `is_blacklisted` flag on the unified `AllowlistEntry` PDA,
//! opening the PDA on first true flag and closing it once both flags clear.
//! EVM parity: AllowlistRBACUpgradeable.setBlacklisted(SetAllowlistParam[])
//!
//! Requires the Blacklister role.

use crate::{
    errors::HookError,
    state::{AllowlistEntry, TransferHookState, ALLOWLIST_SEED},
    BlacklistUpdated, RoleMember, RoleType,
};
use anchor_lang::prelude::*;

/// Toggle a pubkey on/off one side of the unified allowlist entry.
#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct SetAllowlistedParams {
    /// Subject pubkey to mark.
    pub pubkey: Pubkey,
    /// Desired list membership for the instruction being called.
    pub is_enabled: bool,
}

#[rbac::only_role(role = RoleType::Blacklister, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
#[instruction(params: SetAllowlistedParams)]
pub struct SetBlacklisted<'info> {
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
        constraint = allowlist_entry.is_blacklisted != params.is_enabled
            @ HookError::AllowlistStateIdempotent,
    )]
    pub allowlist_entry: Account<'info, AllowlistEntry>,

    pub system_program: Program<'info, System>,
}

impl SetBlacklisted<'_> {
    pub fn apply(ctx: &mut Context<Self>, params: &SetAllowlistedParams) -> Result<()> {
        let entry = &mut ctx.accounts.allowlist_entry;
        entry.is_blacklisted = params.is_enabled;
        AllowlistEntry::close_if_empty(entry, ctx.accounts.payer.to_account_info())?;

        emit_cpi!(BlacklistUpdated {
            mint: ctx.accounts.hook_state.mint,
            pubkey: params.pubkey,
            is_blacklisted: params.is_enabled,
        });

        Ok(())
    }
}
