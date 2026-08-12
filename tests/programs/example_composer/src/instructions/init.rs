use crate::{ComposerStore, COMPOSER_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Init<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + ComposerStore::INIT_SPACE,
        seeds = [COMPOSER_SEED],
        bump,
    )]
    pub composer_store: Account<'info, ComposerStore>,

    pub system_program: Program<'info, System>,
}

impl Init<'_> {
    pub fn apply(ctx: &mut Context<Self>) -> Result<()> {
        ctx.accounts
            .composer_store
            .set_inner(ComposerStore { compose_count: 0, bump: ctx.bumps.composer_store });
        Ok(())
    }
}
