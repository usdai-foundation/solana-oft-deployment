//! Code generator for the `lz_receive_types_v2` instruction.
//!
//! This instruction has no default implementation — users MUST provide
//! a custom handler named `lz_receive_types_v2` with `#[oapp_instruction]`.

use super::CodegenContext;
use anchor_trait::InstructionSpec;
use quote::quote;

pub(super) fn spec() -> InstructionSpec<CodegenContext> {
    InstructionSpec {
        params_type: Some(quote!(::oapp::types::LzReceiveParams)),
        return_type: quote!(::oapp::lz_receive_types_v2::LzReceiveTypesV2Result),
        is_view: true,
        default_impl: None,
    }
}
