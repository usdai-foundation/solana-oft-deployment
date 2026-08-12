use crate::{OFTError, OFTStore, RoleMember, RoleType, MAX_ALTS};
use anchor_lang::prelude::*;
use oapp::OAppError;
use oft::events::AddressLookupTablesSet;
use solana_address_lookup_table_interface::program::ID as ALT_PROGRAM_ID;

#[rbac::only_role(role = RoleType::DefaultAdmin, state = oft_store, authority = authority)]
#[event_cpi]
#[derive(Accounts)]
pub struct SetAlts<'info> {
    pub authority: Signer<'info>,

    #[account(mut)]
    pub oft_store: Account<'info, OFTStore>,
    // ALT accounts are passed via `ctx.remaining_accounts`; each must be owned by ALT_PROGRAM_ID.
}

impl SetAlts<'_> {
    pub fn apply(ctx: &mut Context<SetAlts>) -> Result<()> {
        require!(ctx.remaining_accounts.len() <= MAX_ALTS, OFTError::TooManyAlts);

        let mut alts = Vec::with_capacity(ctx.remaining_accounts.len());
        for account in ctx.remaining_accounts.iter() {
            require_keys_eq!(*account.owner, ALT_PROGRAM_ID, OAppError::InvalidAddressLookupTable);
            alts.push(account.key());
        }

        emit_cpi!(AddressLookupTablesSet {
            oft_store: ctx.accounts.oft_store.key(),
            alts: alts.clone()
        });

        ctx.accounts.oft_store.alts = alts;
        Ok(())
    }
}
