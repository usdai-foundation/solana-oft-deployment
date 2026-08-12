use crate::{common::AccountMetaRef, COMPOSED_MESSAGE_HASH_SEED, EVENT_SEED};
use anchor_lang::prelude::*;
use solana_keccak_hasher::hash;

/// Version identifier returned by `lz_compose_types_info` for the V2 flow.
pub const LZ_COMPOSE_TYPES_VERSION: u8 = 2;

/// Payload returned from `lz_compose_types_info` when the version is V2.
///
/// The executor uses these accounts to construct the follow-up
/// `lz_compose_types_v2` call.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct LzComposeTypesV2Accounts {
    pub accounts: Vec<Pubkey>,
}

/// Output of the lz_compose_types_v2 instruction.
///
/// This structure enables the multi-instruction execution model where composers
/// can define multiple instructions to be executed atomically by the Executor.
/// The Executor constructs a single transaction containing all returned instructions.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LzComposeTypesV2Result {
    /// The version of context account
    pub context_version: u8,
    /// ALTs required for this execution context
    /// Used by the Executor to resolve AltIndex references in AccountMetaRef
    pub alts: Vec<Pubkey>,
    /// The complete list of instructions required for LzCompose execution
    /// MUST include exactly one LzCompose instruction
    /// MAY include additional Standard instructions for preprocessing/postprocessing
    /// Instructions are executed in the order returned
    pub instructions: Vec<ComposeInstruction>,
}

/// The list of instructions that can be executed in the LzCompose transaction.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum ComposeInstruction {
    /// The main LzCompose instruction (exactly one required per transaction)
    /// This instruction processes the incoming composed message
    LzCompose {
        /// Account list for the lz_compose instruction
        /// Uses AddressLocator for flexible address resolution
        accounts: Vec<AccountMetaRef>,
    },
    /// Arbitrary custom instruction for preprocessing/postprocessing
    Standard {
        /// Target program ID for the custom instruction
        program_id: Pubkey,
        /// Account list for the custom instruction
        accounts: Vec<AccountMetaRef>,
        /// Instruction data payload
        data: Vec<u8>,
    },
}

/// V2 version of get_accounts_for_clear_compose that returns AccountMetaRef
pub fn get_accounts_for_clear_compose(
    endpoint_program: Pubkey,
    from: &Pubkey,
    to: &Pubkey,
    guid: &[u8; 32],
    index: u16,
    composed_message: &[u8],
) -> Vec<AccountMetaRef> {
    let (composed_message_account, _) = Pubkey::find_program_address(
        &[
            COMPOSED_MESSAGE_HASH_SEED,
            &from.to_bytes(),
            &to.to_bytes(),
            &guid[..],
            &index.to_be_bytes(),
            &hash(composed_message).to_bytes(),
        ],
        &endpoint_program,
    );

    let (event_authority_account, _) =
        Pubkey::find_program_address(&[EVENT_SEED], &endpoint_program);

    vec![
        AccountMetaRef { pubkey: endpoint_program.into(), is_writable: false },
        AccountMetaRef { pubkey: (*to).into(), is_writable: false },
        AccountMetaRef { pubkey: composed_message_account.into(), is_writable: true },
        AccountMetaRef { pubkey: event_authority_account.into(), is_writable: false },
        AccountMetaRef { pubkey: endpoint_program.into(), is_writable: false },
    ]
}
