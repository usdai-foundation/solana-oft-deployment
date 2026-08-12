//! Set Bypass instruction
//!
//! Toggles a hook-state-scoped bypass entry for the seeded `account` (a token
//! account, not a wallet). When the entry exists, the seeded account bypasses
//! all transfer-hook compliance checks (pause + allowlist) while it is the
//! source of a transfer for this hook state's mint. Matches EVM `mint()`, which
//! has no compliance modifiers and allows OFT receive flows to credit blocked recipients.
//!
//! Requires the `BypassManager` role.

use crate::{
    errors::HookError,
    state::{BypassEntry, TransferHookState, BYPASS_SEED},
    BypassUpdated, RoleMember, RoleType,
};
use anchor_lang::prelude::*;

/// Toggle a `(mint, account)` bypass entry on/off.
#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct SetBypassParams {
    pub account: Pubkey,
    pub is_enabled: bool,
}

#[rbac::only_role(role = RoleType::BypassManager, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
#[instruction(params: SetBypassParams)]
pub struct SetBypass<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub hook_state: Account<'info, TransferHookState>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + BypassEntry::INIT_SPACE,
        seeds = [BYPASS_SEED, hook_state.key().as_ref(), params.account.as_ref()],
        constraint = bypass_entry.initialized != params.is_enabled @ HookError::BypassStateIdempotent,
        bump,
    )]
    pub bypass_entry: Account<'info, BypassEntry>,

    pub system_program: Program<'info, System>,
}

impl SetBypass<'_> {
    pub fn apply(ctx: &mut Context<Self>, params: &SetBypassParams) -> Result<()> {
        let entry = &mut ctx.accounts.bypass_entry;
        if params.is_enabled {
            entry.initialized = true;
        } else {
            ctx.accounts.bypass_entry.close(ctx.accounts.payer.to_account_info())?;
        }

        emit_cpi!(BypassUpdated {
            mint: ctx.accounts.hook_state.mint,
            account: params.account,
            is_enabled: params.is_enabled,
        });

        Ok(())
    }
}
