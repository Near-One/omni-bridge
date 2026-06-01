use bridge_token_factory::state::message::init_transfer::InitTransferPayload;
use mollusk_svm::result::ProgramResult;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_sdk_ids::system_program;

use crate::mollusk::helpers::*;

const TRANSFER_AMOUNT: u128 = 1_000_000;
const NATIVE_FEE: u64 = 100;
const RECIPIENT: &str = "recipient.near";

struct TestParams {
    amount: u128,
    fee: u128,
    native_fee: u64,
    paused: u8,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            amount: TRANSFER_AMOUNT,
            fee: 0,
            native_fee: NATIVE_FEE,
            paused: 0,
        }
    }
}

fn run_init_transfer_sol(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();

    let user = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            paused: params.paused,
            ..Default::default()
        },
    );

    let (sol_vault_pda, _) = find_sol_vault_pda(&program_id);
    let sol_vault_account = Account::new(1_000_000_000, 0, &system_program::ID);

    let user_account = create_signer_account(10_000_000_000);

    let (wormhole_accounts, wormhole_metas) =
        build_wormhole_cpi_accounts(&config_pda, &config_account, &user, &user_account);

    let payload = InitTransferPayload {
        amount: params.amount,
        recipient: RECIPIENT.to_string(),
        fee: params.fee,
        native_fee: params.native_fee,
        message: String::new(),
    };
    let mut ix_data = anchor_ix_discriminator("init_transfer_sol").to_vec();
    anchor_lang::AnchorSerialize::serialize(&payload, &mut ix_data).unwrap();

    let mut metas = vec![
        AccountMeta::new(sol_vault_pda, false),
        AccountMeta::new(user, true),
    ];
    metas.extend(wormhole_metas);

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let mut accounts = vec![
        (sol_vault_pda, sol_vault_account),
        (user, user_account.clone()),
    ];
    accounts.extend(wormhole_accounts);

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn init_transfer_sol_happy_path() {
    let result = run_init_transfer_sol(TestParams::default());

    assert!(
        !result.program_result.is_err(),
        "init_transfer_sol failed: {:?}",
        result.program_result
    );
}

#[test]
fn init_transfer_sol_paused() {
    let result = run_init_transfer_sol(TestParams {
        paused: INIT_TRANSFER_PAUSED,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6008))
    );
}

#[test]
fn init_transfer_sol_nonzero_fee_rejected() {
    let result = run_init_transfer_sol(TestParams {
        fee: 10,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6007))
    );
}
