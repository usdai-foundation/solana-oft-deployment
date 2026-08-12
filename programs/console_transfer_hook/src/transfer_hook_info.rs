use anchor_lang::{
    prelude::{borsh, AccountInfo, AnchorDeserialize, AnchorSerialize},
    Space,
};
use anchor_spl::token_interface::spl_token_metadata_interface::solana_borsh;
use spl_discriminator::SplDiscriminate;
use spl_type_length_value::{state::TlvStateMut, SplBorshVariableLenPack};

pub const TRANSFER_HOOK_INFO_SEED: &[u8] = b"transfer-hook-info";
pub const TRANSFER_HOOK_INFO_SCHEMA_VERSION: u8 = 1;
pub const TRANSFER_HOOK_INFO_IDL_VERSION: IdlVersion = IdlVersion { major: 1, minor: 0 };

#[derive(Clone, Copy, Debug, PartialEq, Eq, AnchorSerialize, AnchorDeserialize)]
pub struct IdlVersion {
    pub major: u8,
    pub minor: u8,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    AnchorSerialize,
    AnchorDeserialize,
    SplDiscriminate,
    SplBorshVariableLenPack,
)]
#[discriminator_hash_input("console-transfer-hook::info")]
pub struct TransferHookInfo {
    pub schema_version: u8,
    pub idl_version: IdlVersion,
}

impl Space for TransferHookInfo {
    /// Raw SPL TLV account size. This account has no Anchor discriminator:
    /// 8-byte TLV type discriminator + 4-byte TLV length + 3-byte Borsh payload
    /// (`schema_version`, `idl_version.major`, `idl_version.minor`).
    const INIT_SPACE: usize = 15;
}

impl Default for TransferHookInfo {
    fn default() -> Self {
        Self {
            schema_version: TRANSFER_HOOK_INFO_SCHEMA_VERSION,
            idl_version: TRANSFER_HOOK_INFO_IDL_VERSION,
        }
    }
}

pub fn init_transfer_hook_info(info: &AccountInfo) -> anchor_lang::Result<()> {
    let mut raw = info.try_borrow_mut_data()?;
    let mut state = TlvStateMut::unpack(&mut raw[..])?;
    state.alloc_and_pack_variable_len_entry(&TransferHookInfo::default(), false)?;
    Ok(())
}
