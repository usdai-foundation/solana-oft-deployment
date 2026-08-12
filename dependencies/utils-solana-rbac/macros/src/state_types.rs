//! State struct generators for the `#[rbac]` macro.
//!
//! Contains token-stream generators for RBAC state account types:
//! - `RoleMember` — proves an account holds a specific role (PDA existence = hasRole).
//! - `DefaultAdmin` derive — validates and implements the DefaultAdmin trait on the root account.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::utils;

/// Generates the `RoleMember` account struct parameterised by the caller's role type.
///
/// The `role` and `account` fields are technically redundant since they're already embedded
/// in the PDA seeds. However, storing them in the account data enables enumeration via
/// `getProgramAccounts` RPC calls with filters.
///
/// **IMPORTANT**: Field ordering matters for RPC filtering!
/// - `role` MUST be the first field (offset 8, after 8-byte Anchor discriminator)
/// - This allows efficient filtering by role type using `memcmp` at a fixed offset
///
/// Example RPC query to enumerate all members of a specific role:
/// ```json
/// {
///   "method": "getProgramAccounts",
///   "params": [
///     "<PROGRAM_ID>",
///     {
///       "filters": [
///         { "dataSize": 42 },  // 8 (discriminator) + 1 (role) + 32 (account) + 1 (bump)
///         { "memcmp": { "offset": 0, "bytes": "<DISCRIMINATOR_AND_ROLE_BASE58>" } }
///       ]
///     }
///   ]
/// }
///
/// Note: The memcmp bytes combine discriminator (8 bytes) + role (1 byte) = 9 bytes total.
///
/// This achieves similar functionality to EVM's AccessControlEnumerable:
/// - EVM: getRoleMember(role, index) - O(1) lookup by index
/// - Solana: getProgramAccounts with filters - returns all members at once
pub fn generate_role_member(role_type: &TokenStream) -> TokenStream {
    quote! {
        #[account]
        #[derive(Default, InitSpace)]
        pub struct RoleMember {
            /// The role type. MUST be first field for RPC enumeration filtering at offset 8.
            pub role: #role_type,
            /// The account's public key who holds this role.
            pub account: Pubkey,
            /// PDA bump seed.
            pub bump: u8,
        }
    }
}

/// Derive macro implementation for `DefaultAdmin` trait.
///
/// Validates that the struct has `current_default_admin` and `pending_default_admin`
/// fields, then generates `impl ::rbac::traits::DefaultAdmin for T`.
pub fn generate_default_admin_derive(input: TokenStream) -> syn::Result<TokenStream> {
    let input = syn::parse2::<DeriveInput>(input)?;
    let name = &input.ident;
    utils::validate_named_struct(
        &input,
        "DefaultAdmin",
        &["current_default_admin", "pending_default_admin"],
    )?;

    Ok(quote! {
        impl ::rbac::traits::DefaultAdmin for #name {
            fn set_pending_default_admin(&mut self, admin: Pubkey) {
                self.pending_default_admin = admin;
            }

            fn pending_default_admin(&self) -> &Pubkey {
                &self.pending_default_admin
            }

            fn set_current_default_admin(&mut self, admin: Pubkey) {
                self.current_default_admin = admin;
            }

            fn current_default_admin(&self) -> &Pubkey {
                &self.current_default_admin
            }
        }
    })
}
