use crate::ComposerStore;
use anchor_lang::prelude::*;
use oapp::{
    common::{AccountMetaRef, EXECUTION_CONTEXT_VERSION_1},
    endpoint::ID as ENDPOINT_ID,
    lz_compose_types_v2::{
        get_accounts_for_clear_compose, ComposeInstruction, LzComposeTypesV2Result,
    },
    types::LzComposeParams,
};

#[derive(Accounts)]
#[instruction(params: LzComposeParams)]
pub struct LzComposeTypesV2<'info> {
    #[account(constraint = composer_store.key() == params.to)]
    pub composer_store: Account<'info, ComposerStore>,
}

impl LzComposeTypesV2<'_> {
    pub fn apply(ctx: &Context<Self>, params: &LzComposeParams) -> Result<LzComposeTypesV2Result> {
        let mut accounts = vec![AccountMetaRef {
            pubkey: ctx.accounts.composer_store.key().into(),
            is_writable: true,
        }];

        accounts.extend(get_accounts_for_clear_compose(
            ENDPOINT_ID,
            &params.from,
            &params.to,
            &params.guid,
            params.index,
            &params.message,
        ));

        Ok(LzComposeTypesV2Result {
            context_version: EXECUTION_CONTEXT_VERSION_1,
            alts: vec![],
            instructions: vec![ComposeInstruction::LzCompose { accounts }],
        })
    }
}
