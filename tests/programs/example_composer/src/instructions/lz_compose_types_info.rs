use crate::{ComposerStore, COMPOSER_SEED};
use anchor_lang::prelude::*;
use oapp::{
    lz_compose_types_v2::{LzComposeTypesV2Accounts, LZ_COMPOSE_TYPES_VERSION},
    types::LzComposeParams,
};

#[derive(Accounts)]
#[instruction(params: LzComposeParams)]
pub struct LzComposeTypesInfo<'info> {
    #[account(seeds = [COMPOSER_SEED], bump = composer_store.bump)]
    pub composer_store: Account<'info, ComposerStore>,

    /// CHECK: PDA for lz_compose_types_v2 lookup
    #[account(seeds = [oapp::LZ_COMPOSE_TYPES_SEED, &composer_store.key().to_bytes()], bump)]
    pub lz_compose_types_accounts: UncheckedAccount<'info>,
}

impl LzComposeTypesInfo<'_> {
    pub fn apply(
        ctx: &Context<Self>,
        params: &LzComposeParams,
    ) -> Result<(u8, LzComposeTypesV2Accounts)> {
        Ok((
            LZ_COMPOSE_TYPES_VERSION,
            LzComposeTypesV2Accounts {
                accounts: vec![ctx.accounts.composer_store.key(), params.from],
            },
        ))
    }
}
