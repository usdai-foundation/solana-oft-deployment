//! Code generator for the `lz_compose_types_v2` instruction.
//!
//! This instruction has no default implementation — users MUST provide
//! a custom handler named `lz_compose_types_v2` with `#[composer_instruction]`.

use super::ComposerCodegenContext;
use anchor_trait::InstructionSpec;
use quote::quote;

pub(super) fn spec() -> InstructionSpec<ComposerCodegenContext> {
    InstructionSpec {
        params_type: Some(quote!(::oapp::types::LzComposeParams)),
        return_type: quote!(::oapp::lz_compose_types_v2::LzComposeTypesV2Result),
        is_view: true,
        default_impl: None,
    }
}
