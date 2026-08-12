//! Unpause instruction — EVM: PauseRBACUpgradeable.unpause
//!
//! Clears the per-mint pause bit on `TransferHookState`.
//! Requires Unpauser role. Read via `is_paused`.

use crate::{errors::HookError, state::TransferHookState, PauseSet, RoleMember, RoleType};
use anchor_lang::prelude::*;

#[rbac::only_role(role = RoleType::Unpauser, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
pub struct Unpause<'info> {
    pub authority: Signer<'info>,

    #[account(mut, constraint = hook_state.paused @ HookError::PauseStateIdempotent)]
    pub hook_state: Account<'info, TransferHookState>,
}

impl Unpause<'_> {
    pub fn apply(ctx: &mut Context<Self>) -> Result<()> {
        ctx.accounts.hook_state.paused = false;
        emit_cpi!(PauseSet { mint: ctx.accounts.hook_state.mint, paused: false });
        Ok(())
    }
}
