#[path = "transfer_hook/helpers.rs"]
mod helpers;

use anchor_lang::error::ErrorCode;
use console_transfer_hook::transfer_hook_info::{
    TransferHookInfo, TRANSFER_HOOK_INFO_IDL_VERSION, TRANSFER_HOOK_INFO_SCHEMA_VERSION,
};
use mollusk_svm_result::Check;
use solana_pubkey::Pubkey;
use spl_type_length_value::state::{TlvState, TlvStateBorrowed};

use crate::helpers::*;

fn read_transfer_hook_info(context: &TestContext, transfer_hook_info: &Pubkey) -> TransferHookInfo {
    let account = read_raw_account(context, transfer_hook_info);
    let state =
        TlvStateBorrowed::unpack(&account.data).expect("invalid TransferHookInfo TLV state");
    state
        .get_first_variable_len_value::<TransferHookInfo>()
        .expect("missing TransferHookInfo")
}

#[test]
fn init_transfer_hook_info_creates_program_level_info_pda() {
    let context = context();
    let payer = payer_key();
    let (transfer_hook_info, _) = transfer_hook_info_pda();

    insert_wallet(&context, payer);

    context.process_and_validate_instruction(
        &init_transfer_hook_info_ix(&payer, &transfer_hook_info),
        &[
            Check::success(),
            Check::account(&transfer_hook_info).owner(&program_id()).rent_exempt().build(),
        ],
    );

    let info = read_transfer_hook_info(&context, &transfer_hook_info);
    assert_eq!(info.schema_version, TRANSFER_HOOK_INFO_SCHEMA_VERSION);
    assert_eq!(info.idl_version, TRANSFER_HOOK_INFO_IDL_VERSION);
}

#[test]
fn init_transfer_hook_info_rejects_wrong_info_pda() {
    let context = context();
    let payer = payer_key();
    let wrong_transfer_hook_info = Pubkey::new_from_array([0xCE; 32]);

    insert_wallet(&context, payer);

    let result =
        context.process_instruction(&init_transfer_hook_info_ix(&payer, &wrong_transfer_hook_info));

    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(program_err(ErrorCode::ConstraintSeeds))
    );
}
