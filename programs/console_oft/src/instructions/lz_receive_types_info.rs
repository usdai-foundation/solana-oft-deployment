use crate::{codec, seeds::EXTRA_ACCOUNT_METAS_SEED, OFTStore};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use oapp::{
    lz_receive_types_seeds,
    lz_receive_types_v2::{LzReceiveTypesV2Accounts, LZ_RECEIVE_TYPES_VERSION},
    types::LzReceiveParams,
};

/// Step 1 of the lz_receive_types_v2 flow:
/// - Returns (version, minimal accounts) needed to call `lz_receive_types_v2`.
#[derive(Accounts)]
pub struct LzReceiveTypesInfo<'info> {
    pub oft_store: Account<'info, OFTStore>,

    /// CHECK: lz_receive_types_accounts account
    #[account(
        seeds = lz_receive_types_seeds(&oft_store.key()),
        bump
    )]
    pub lz_receive_types_accounts: UncheckedAccount<'info>,
}

impl LzReceiveTypesInfo<'_> {
    pub fn apply(
        ctx: &Context<LzReceiveTypesInfo>,
        params: &LzReceiveParams,
    ) -> Result<(u8, LzReceiveTypesV2Accounts)> {
        // Minimal account set for calling lz_receive_types_v2.
        // V2 reads and validates token_mint for mint state, decimals, and transfer-hook metadata.
        // token_program is supplied from oft_store.token_program for ATA derivation.
        let token_mint = ctx.accounts.oft_store.token_mint;
        let (transfer_hook_program, extra_account_meta_list) =
            if ctx.accounts.oft_store.transfer_hook_program != Pubkey::default() {
                let transfer_hook_program = ctx.accounts.oft_store.transfer_hook_program;
                let (extra_account_meta_list, _bump) = Pubkey::find_program_address(
                    &[EXTRA_ACCOUNT_METAS_SEED, token_mint.as_ref()],
                    &transfer_hook_program,
                );
                (transfer_hook_program, extra_account_meta_list)
            } else {
                // Anchor treats the current program ID as the sentinel for `None`
                // when deserializing optional accounts. LzReceiveTypesV2 uses these
                // placeholders for the optional transfer-hook accounts.
                (crate::ID, crate::ID)
            };
        let to_address = Pubkey::from(codec::msg::send_to(&params.message)?);
        let token_dest = get_associated_token_address_with_program_id(
            &to_address,
            &ctx.accounts.oft_store.token_mint,
            &ctx.accounts.oft_store.token_program,
        );
        let (token_escrow, _) = Pubkey::find_program_address(
            OFTStore::token_escrow_seeds(&ctx.accounts.oft_store.key()),
            &crate::ID,
        );
        // Order must match LzReceiveTypesV2 struct:
        // oft_store, token_escrow, token_mint, token_dest, token_program, hooks...
        let mut required_accounts = vec![
            ctx.accounts.oft_store.key(),
            token_escrow,
            ctx.accounts.oft_store.token_mint.key(),
            token_dest,
            ctx.accounts.oft_store.token_program,
            transfer_hook_program,
            extra_account_meta_list,
        ];

        // If any Address Lookup Tables (ALTs) are configured in oft_store, include them.
        // These ALTs will be passed via `remaining_accounts` in LzReceiveTypesV2,
        // where they're used by `compact_accounts_with_alts` to compress the instruction accounts.
        required_accounts.extend(ctx.accounts.oft_store.alts.iter().copied());
        Ok((LZ_RECEIVE_TYPES_VERSION, LzReceiveTypesV2Accounts { accounts: required_accounts }))
    }
}
