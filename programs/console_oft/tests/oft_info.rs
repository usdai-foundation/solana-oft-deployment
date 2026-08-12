mod oft;

use ::oft::oft_info::{OftInfo, OFT_INFO_SCHEMA_VERSION};
use anchor_lang::error::ErrorCode;
use console_oft::console_oft_info::{
    ConsoleOftInfo, CONSOLE_OFT_INFO_IDL_VERSION, CONSOLE_OFT_INFO_SCHEMA_VERSION,
};
use mollusk_svm_result::Check;
use oapp::oapp_info::{OAppBase, OAPP_BASE_IDL_VERSION, OAPP_BASE_SCHEMA_VERSION};
use solana_pubkey::Pubkey;
use spl_discriminator::SplDiscriminate;
use spl_type_length_value::state::{TlvState, TlvStateBorrowed};

use crate::oft::helpers::*;

fn read_tlv_entry<T>(context: &TestContext, oapp_info: &Pubkey) -> T
where
    T: SplDiscriminate + spl_type_length_value::variable_len_pack::VariableLenPack,
{
    let account = read_raw_account(context, oapp_info);
    let state = TlvStateBorrowed::unpack(&account.data).expect("invalid OAppInfo TLV state");
    state.get_first_variable_len_value::<T>().expect("missing OAppInfo TLV entry")
}

#[test]
fn init_console_oft_info_creates_oapp_info_tlv_chain() {
    let context = context();
    let payer = payer_key();
    let (oapp_info, _) = oapp_info_pda();

    insert_wallet(&context, payer);

    context.process_and_validate_instruction(
        &init_console_oft_info_ix(&payer, &oapp_info),
        &[Check::success(), Check::account(&oapp_info).owner(&program_id()).rent_exempt().build()],
    );

    let oapp_base = read_tlv_entry::<OAppBase>(&context, &oapp_info);
    let oft_info = read_tlv_entry::<OftInfo>(&context, &oapp_info);
    let console_info = read_tlv_entry::<ConsoleOftInfo>(&context, &oapp_info);

    assert_eq!(oapp_base.schema_version, OAPP_BASE_SCHEMA_VERSION);
    assert_eq!(oapp_base.idl_version, OAPP_BASE_IDL_VERSION);
    assert_eq!(oapp_base.app_discriminator, <[u8; 8]>::from(OftInfo::SPL_DISCRIMINATOR));

    assert_eq!(oft_info.schema_version, OFT_INFO_SCHEMA_VERSION);
    assert_eq!(oft_info.project_discriminator, <[u8; 8]>::from(ConsoleOftInfo::SPL_DISCRIMINATOR));

    assert_eq!(console_info.schema_version, CONSOLE_OFT_INFO_SCHEMA_VERSION);
    assert_eq!(console_info.idl_version, CONSOLE_OFT_INFO_IDL_VERSION);
}

#[test]
fn init_console_oft_info_rejects_wrong_oapp_info_pda() {
    let context = context();
    let payer = payer_key();
    let wrong_oapp_info = Pubkey::new_from_array([0xCD; 32]);

    insert_wallet(&context, payer);

    let result = context.process_instruction(&init_console_oft_info_ix(&payer, &wrong_oapp_info));

    assert_eq!(result.program_result, program_failure(ErrorCode::ConstraintSeeds));
}
