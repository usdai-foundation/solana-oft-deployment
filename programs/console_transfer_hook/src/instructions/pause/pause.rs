//! Pause instruction — EVM: PauseRBACUpgradeable.pause
//!
//! Sets the per-mint pause bit on `TransferHookState`.
//! Requires Pauser role. Read via `is_paused`.

use crate::{errors::HookError, state::TransferHookState, PauseSet, RoleMember, RoleType};
use anchor_lang::prelude::*;

#[rbac::only_role(role = RoleType::Pauser, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
pub struct Pause<'info> {
    pub authority: Signer<'info>,

    #[account(mut, constraint = !hook_state.paused @ HookError::PauseStateIdempotent)]
    pub hook_state: Account<'info, TransferHookState>,
}

impl Pause<'_> {
    pub fn apply(ctx: &mut Context<Self>) -> Result<()> {
        ctx.accounts.hook_state.paused = true;
        emit_cpi!(PauseSet { mint: ctx.accounts.hook_state.mint, paused: true });
        Ok(())
    }
}
