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
const DECIMALS: u8 = 6;

struct TestParams {
    amount: u128,
    fee: u128,
    native_fee: u64,
    paused: u8,
    /// true = native token (vault exists), false = bridged token (burn)
    native_token: bool,
    /// For bridged token: use wrong mint authority
    wrong_mint_authority: bool,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            amount: TRANSFER_AMOUNT,
            fee: 0,
            native_fee: NATIVE_FEE,
            paused: 0,
            native_token: true,
            wrong_mint_authority: false,
        }
    }
}

fn run_init_transfer(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();

    let user = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = anchor_spl::token::ID;

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            paused: params.paused,
            ..Default::default()
        },
    );

    let (authority_pda, _) = find_authority_pda(&program_id);
    let authority_account = Account::new(0, 0, &system_program::ID);

    // Create mint: for bridged tokens, mint_authority = authority PDA
    let mint_authority = if !params.native_token && !params.wrong_mint_authority {
        authority_pda
    } else {
        Pubkey::new_unique()
    };
    let mint_account = create_mint_account(Some(&mint_authority), 1_000_000_000, DECIMALS);

    let from = Pubkey::new_unique();
    let from_account = create_token_account(&mint, &user, 10_000_000);

    // Vault: Some for native tokens, program_id sentinel for None (bridged)
    let (vault_key, vault_account) = if params.native_token {
        let (vault_pda, _) = find_vault_pda(&program_id, &mint);
        (vault_pda, create_token_account(&mint, &authority_pda, 1_000_000_000))
    } else {
        (program_id, create_program_account())
    };

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
    let mut ix_data = anchor_ix_discriminator("init_transfer").to_vec();
    anchor_lang::AnchorSerialize::serialize(&payload, &mut ix_data).unwrap();

    let mut metas = vec![
        AccountMeta::new_readonly(authority_pda, false),
        AccountMeta::new(mint, false),
        AccountMeta::new(from, false),
        AccountMeta::new(vault_key, false),
        AccountMeta::new(sol_vault_pda, false),
        AccountMeta::new(user, true),
    ];
    metas.extend(wormhole_metas);
    metas.push(AccountMeta::new_readonly(token_program, false));

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let mut accounts = vec![
        (authority_pda, authority_account),
        (mint, mint_account),
        (from, from_account),
        (vault_key, vault_account),
        (sol_vault_pda, sol_vault_account),
        (user, user_account.clone()),
    ];
    accounts.extend(wormhole_accounts);
    accounts.push((token_program, create_program_account()));

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn init_transfer_native_happy_path() {
    let result = run_init_transfer(TestParams {
        native_token: true,
        ..Default::default()
    });

    assert!(
        !result.program_result.is_err(),
        "init_transfer (native) failed: {:?}",
        result.program_result
    );
}

#[test]
fn init_transfer_bridged_happy_path() {
    let result = run_init_transfer(TestParams {
        native_token: false,
        native_fee: 0,
        ..Default::default()
    });

    assert!(
        !result.program_result.is_err(),
        "init_transfer (bridged) failed: {:?}",
        result.program_result
    );
}

#[test]
fn init_transfer_paused() {
    let result = run_init_transfer(TestParams {
        paused: INIT_TRANSFER_PAUSED,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6008))
    );
}

#[test]
fn init_transfer_fee_gte_amount_rejected() {
    let result = run_init_transfer(TestParams {
        amount: 100,
        fee: 100,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6007))
    );
}

#[test]
fn init_transfer_invalid_bridged_token() {
    let result = run_init_transfer(TestParams {
        native_token: false,
        wrong_mint_authority: true,
        native_fee: 0,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6006))
    );
}
