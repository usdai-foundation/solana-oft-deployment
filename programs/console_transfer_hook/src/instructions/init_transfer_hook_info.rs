use crate::transfer_hook_info::{
    init_transfer_hook_info, TransferHookInfo, TRANSFER_HOOK_INFO_SEED,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitTransferHookInfo<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Raw SPL TLV account — no Anchor discriminator. Address verified
    /// by PDA seeds; ownership and allocation enforced by `init`.
    #[account(
        init,
        payer = payer,
        space = TransferHookInfo::INIT_SPACE,
        seeds = [TRANSFER_HOOK_INFO_SEED],
        bump,
    )]
    pub transfer_hook_info: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl InitTransferHookInfo<'_> {
    pub fn apply(ctx: &mut Context<Self>) -> Result<()> {
        init_transfer_hook_info(&ctx.accounts.transfer_hook_info.to_account_info())
    }
}
