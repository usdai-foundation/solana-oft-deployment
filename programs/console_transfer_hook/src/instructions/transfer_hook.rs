//! Transfer Hook instruction
//!
//! Called by Token-2022 on every transfer. Performs compliance checks:
//! 1. Fund recovery bypass - PermanentDelegate recovering from blocked source accounts
//! 2. Mint bypass - bypassed accounts (OFT escrows) skip all checks
//! 3. Pause check - blocks transfers when paused
//! 4. Allowlist check - blocks based on blacklist/whitelist mode
//!
//! # Fund Recovery (EVM parity: `recoverFunds`)
//!
//! PermanentDelegate rules:
//! - PD transferring from a blocked source account -> bypass all checks (fund recovery)
//! - PD transferring from a non-blocked account they own/control -> normal checks apply
//! - PD transferring from a non-blocked account they do not control -> blocked
//!
//! # Mint Bypass (EVM parity: `mint()` has no modifiers)
//!
//! Accounts placed on the bypass list via `set_bypass` (typically OFT escrows)
//! bypass all checks when they are the source. This matches EVM where `mint()`
//! has neither `whenNotPaused` nor `onlyAllowlisted` modifiers.
//!
//! This means:
//! - Blocked users CAN receive cross-chain inbound tokens through OFT escrow bypass
//! - Cross-chain inbound works even when paused
//! - Bypass is mint-scoped
//!
//! # Allowlist Checks
//!
//! The hook checks three pubkeys:
//! - `from`: source token account
//! - `to`: destination token account
//! - `authority`: signer pubkey with control over the funds

use crate::{
    errors::HookError,
    state::{AllowlistEntry, TransferHookState, ALLOWLIST_SEED, BYPASS_SEED, TRANSFER_HOOK_SEED},
};
use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl_token_2022::{
    extension::{
        permanent_delegate::PermanentDelegate, BaseStateWithExtensions, StateWithExtensions,
    },
    state::Mint as MintState,
};

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub struct TransferHookParams {
    pub amount: u64,
}

/// Accounts for the transfer hook execution
///
/// Token-2022 passes these accounts in a specific order defined by the
/// Transfer Hook interface. Our extra accounts are appended based on
/// what we registered in ExtraAccountMetaList during `init_transfer_hook`.
///
/// Standard accounts (0-4):
///   0: source token account
///   1: mint
///   2: destination token account
///   3: authority (source owner, approved delegate, or permanent delegate)
///   4: extra_account_meta_list
///
/// Extra accounts (5+):
///   5: hook_state
///   6: source_bypass_entry      (derived from hook_state + source)
///   7: source_allowlist_entry   (derived from hook_state + source token account)
///   8: dest_allowlist_entry     (derived from hook_state + destination token account)
///   9: authority_allowlist_entry (derived from hook_state + authority key)
#[derive(Accounts)]
pub struct TransferHook<'info> {
    // Standard Transfer Hook accounts (order matters!)
    /// Source token account - InterfaceAccount validates Token-2022 account layout
    pub source: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// Destination token account - InterfaceAccount validates Token-2022 account layout
    pub destination: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Transfer authority validated by Token-2022.
    /// This may be the source owner, an approved delegate, or the permanent delegate.
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Extra account meta list PDA
    pub extra_account_meta_list: UncheckedAccount<'info>,

    // Our extra accounts (registered in ExtraAccountMetaList)
    // These are automatically resolved by Token-2022 based on our ExtraAccountMeta definitions
    /// Mint config - contains mint pause state and allowlist mode
    #[account(
        seeds = [TRANSFER_HOOK_SEED, mint.key().as_ref()],
        bump = hook_state.bump
    )]
    pub hook_state: Account<'info, TransferHookState>,

    /// CHECK: Existence-based bypass marker for the source token account.
    /// When this PDA exists with data, the source bypasses pause and allowlist
    /// checks.
    #[account(
        seeds = [BYPASS_SEED, hook_state.key().as_ref(), source.key().as_ref()],
        bump
    )]
    pub source_bypass_entry: UncheckedAccount<'info>,

    /// CHECK: Unified allowlist marker for the source token account.
    #[account(
        seeds = [ALLOWLIST_SEED, hook_state.key().as_ref(), source.key().as_ref()],
        bump
    )]
    pub source_allowlist_entry: UncheckedAccount<'info>,

    /// CHECK: Unified allowlist marker for the destination token account.
    #[account(
        seeds = [ALLOWLIST_SEED, hook_state.key().as_ref(), destination.key().as_ref()],
        bump
    )]
    pub dest_allowlist_entry: UncheckedAccount<'info>,

    /// CHECK: Unified allowlist marker for the authority (signer).
    #[account(
        seeds = [ALLOWLIST_SEED, hook_state.key().as_ref(), authority.key().as_ref()],
        bump
    )]
    pub authority_allowlist_entry: UncheckedAccount<'info>,
}

impl TransferHook<'_> {
    pub fn apply(ctx: &Context<Self>, _params: &TransferHookParams) -> Result<()> {
        // 1. PD fund recovery — short-circuits the hook for recoverFunds().
        if Self::try_bypass_for_fund_recovery(ctx)? {
            return Ok(());
        }

        // 2. Source bypass — mirrors EVM mint() (no modifiers) when the transfer source is
        // explicitly bypassed.
        // For OFT flows, the escrow is usually bypassed, which covers lz_receive
        // transfers because escrow is the source. On send, the escrow is the
        // destination, so user -> escrow transfers still go through pause/allowlist checks.
        if !ctx.accounts.source_bypass_entry.data_is_empty() {
            return Ok(());
        }

        // 3. Pause — mint-scoped state.
        if ctx.accounts.hook_state.paused {
            return Err(HookError::Paused.into());
        }

        // 4. Allowlist (source, dest, authority).
        Self::check_allowlist(ctx)
    }

    /// Attempt to bypass checks for PermanentDelegate fund recovery.
    ///
    /// PD can use their special privilege to recover funds from blocked source accounts.
    /// If the source account is not blocked, PD transfers only continue as regular
    /// transfers when the PD also owns or delegates the source account.
    /// EVM parity: `recoverFunds(from, to, amount)`
    ///
    /// Returns:
    /// - `Ok(true)` - PD is recovering funds from a blocked source account
    /// - `Ok(false)` - Not a PD recovery scenario (continue normal checks)
    /// - `Err` - PD is misusing privilege (transferring from a non-blocked source account)
    fn try_bypass_for_fund_recovery(ctx: &Context<Self>) -> Result<bool> {
        let signer = ctx.accounts.authority.key();

        let mint_info = ctx.accounts.mint.to_account_info();
        let mint_data = mint_info.try_borrow_data()?;
        // Only the mint's permanent delegate can enter the recovery path.
        if get_permanent_delegate(&mint_data) != Some(signer) {
            return Ok(false);
        }

        // Blocked sources are recoverable by PD and bypass the remaining hook checks.
        if Self::is_blocked(&ctx.accounts.hook_state, &ctx.accounts.source_allowlist_entry)? {
            return Ok(true);
        }

        // Non-blocked sources are regular transfers only if PD is the owner or approved delegate.
        if ctx.accounts.source.owner == signer
            || ctx.accounts.source.delegate == COption::Some(signer)
        {
            return Ok(false);
        }

        Err(HookError::CannotRecoverFromAllowlisted.into())
    }

    /// Check source token account, destination token account, and signer against the allowlist.
    fn check_allowlist(ctx: &Context<Self>) -> Result<()> {
        let hook_state = &ctx.accounts.hook_state;
        let accounts = &ctx.accounts;
        for (entry, err) in [
            (&accounts.source_allowlist_entry, HookError::SourceBlocked),
            (&accounts.dest_allowlist_entry, HookError::DestinationBlocked),
            (&accounts.authority_allowlist_entry, HookError::AuthorityBlocked),
        ] {
            if Self::is_blocked(hook_state, entry)? {
                return Err(err.into());
            }
        }
        Ok(())
    }

    /// True when the subject is blocked under the current allowlist mode.
    fn is_blocked(
        hook_state: &Account<'_, TransferHookState>,
        allowlist_entry: &UncheckedAccount,
    ) -> Result<bool> {
        let (blacklisted, whitelisted) = AllowlistEntry::read_flags(allowlist_entry)?;
        Ok(!hook_state.allowlist_mode.allows(blacklisted, whitelisted))
    }
}

/// Get the permanent delegate from a Token-2022 mint's PermanentDelegate extension.
///
/// All "no PD" cases (extension missing, delegate unset, mint data fails to
/// unpack) collapse to `None`.
fn get_permanent_delegate(mint_data: &[u8]) -> Option<Pubkey> {
    StateWithExtensions::<MintState>::unpack(mint_data).ok().and_then(|state| {
        state
            .get_extension::<PermanentDelegate>()
            .ok()
            .and_then(|ext| ext.delegate.into())
    })
}
