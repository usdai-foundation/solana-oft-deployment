//! Code generator for the `lz_receive_types_info` instruction.
//!
//! Required override (no default). Users MUST provide a custom handler
//! named `lz_receive_types_info` with `#[oapp_instruction]`.
//!
//! The off-chain LayerZero Executor invokes this instruction with a fixed
//! 2-account list, in this order:
//!   1. `oapp_account` — the program-specific OApp store
//!   2. `lz_receive_types_accounts` PDA at `[LZ_RECEIVE_TYPES_SEED, oapp_account.key()]`
//!
//! Custom handlers MUST declare an `Accounts` struct matching this shape.
//! The framework does not enforce it at compile time, so a mismatch only
//! surfaces as a runtime failure when the Executor calls the deployed program.
//!
//! See: <https://docs.layerzero.network/v2/developers/solana/oapp/overview#how-lz_receive_types_v2-works>

use super::CodegenContext;
use anchor_trait::InstructionSpec;
use quote::quote;

pub(super) fn spec() -> InstructionSpec<CodegenContext> {
    InstructionSpec {
        params_type: Some(quote!(::oapp::types::LzReceiveParams)),
        return_type: quote!((u8, ::oapp::lz_receive_types_v2::LzReceiveTypesV2Accounts)),
        is_view: true,
        default_impl: None,
    }
}
