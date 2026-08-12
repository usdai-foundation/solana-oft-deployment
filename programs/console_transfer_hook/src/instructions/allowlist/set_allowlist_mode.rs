//! Set Allowlist Mode instruction
//!
//! Changes between Open/Blacklist/Whitelist modes.
//!
//! Requires the DefaultAdmin role (EVM parity: `onlyRole(DEFAULT_ADMIN_ROLE)`).
//!
//! # EVM Parity
//!
//! ```solidity
//! function setAllowlistMode(AllowlistMode _mode) public virtual onlyRole(DEFAULT_ADMIN_ROLE)
//! ```

use crate::{
    errors::HookError, state::TransferHookState, AllowlistMode, AllowlistModeUpdated, RoleMember,
    RoleType,
};
use anchor_lang::prelude::*;

#[rbac::only_role(role = RoleType::DefaultAdmin, state = hook_state, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
#[instruction(new_mode: AllowlistMode)]
pub struct SetAllowlistMode<'info> {
    pub authority: Signer<'info>,

    #[account(mut, constraint = hook_state.allowlist_mode != new_mode @ HookError::ModeAlreadySet)]
    pub hook_state: Account<'info, TransferHookState>,
}

impl SetAllowlistMode<'_> {
    pub fn apply(ctx: &mut Context<Self>, new_mode: AllowlistMode) -> Result<()> {
        ctx.accounts.hook_state.allowlist_mode = new_mode;
        emit_cpi!(AllowlistModeUpdated { mint: ctx.accounts.hook_state.mint, new_mode });
        Ok(())
    }
}
