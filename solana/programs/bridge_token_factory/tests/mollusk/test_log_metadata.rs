use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_sdk_ids::system_program;
use mollusk_svm::result::ProgramResult;

use crate::mollusk::helpers::*;

const DECIMALS: u8 = 6;

struct TestParams {
    /// If true, mint_authority = authority PDA (bridged token → rejected by constraint)
    bridged_token: bool,
    /// If true, metadata account is provided. If false, pass program_id sentinel (None).
    provide_metadata: bool,
    /// If true, use wrong address for metadata (not the metaplex PDA)
    wrong_metadata_address: bool,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            bridged_token: false,
            provide_metadata: true,
            wrong_metadata_address: false,
        }
    }
}

fn run_log_metadata(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();

    let payer = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = anchor_spl::token::ID;

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams::default(),
    );

    let (authority_pda, _) = find_authority_pda(&program_id);
    let authority_account = Account::new(0, 0, &system_program::ID);

    // Create mint: bridged tokens have authority PDA as mint_authority
    let mint_authority = if params.bridged_token {
        authority_pda
    } else {
        Pubkey::new_unique() // native token: authority != bridge authority
    };
    let mint_account = create_mint_account(Some(&mint_authority), 1_000_000_000, DECIMALS);

    // Metadata account (Optional)
    let (metadata_key, metadata_account) = if !params.provide_metadata {
        // None sentinel: use bridge program ID
        (program_id, create_program_account())
    } else if params.wrong_metadata_address {
        // Wrong address: use a random key instead of metaplex PDA
        let wrong_key = Pubkey::new_unique();
        (wrong_key, Account::new(1_000_000, 0, &system_program::ID))
    } else {
        // Correct metaplex PDA, but owned by system_program (returns empty name/symbol)
        let (metadata_pda, _) = find_metaplex_metadata_pda(&mint);
        (metadata_pda, Account::new(1_000_000, 0, &system_program::ID))
    };

    let (vault_pda, _) = find_vault_pda(&program_id, &mint);
    let vault_account = create_token_account(&mint, &authority_pda, 0);

    let payer_account = create_signer_account(10_000_000_000);

    let (wormhole_accounts, wormhole_metas) =
        build_wormhole_cpi_accounts(&config_pda, &config_account, &payer, &payer_account);

    let ix_data = anchor_ix_discriminator("log_metadata").to_vec();

    let mut metas = vec![
        AccountMeta::new_readonly(authority_pda, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new_readonly(metadata_key, false),
        AccountMeta::new(vault_pda, false),
    ];
    metas.extend(wormhole_metas);
    metas.push(AccountMeta::new_readonly(system_program::ID, false));
    metas.push(AccountMeta::new_readonly(token_program, false));
    metas.push(AccountMeta::new_readonly(anchor_spl::associated_token::ID, false));

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let mut accounts = vec![
        (authority_pda, authority_account),
        (mint, mint_account),
        (metadata_key, metadata_account),
        (vault_pda, vault_account),
    ];
    accounts.extend(wormhole_accounts);
    accounts.push((system_program::ID, create_native_program_account()));
    accounts.push((token_program, create_program_account()));
    accounts.push((anchor_spl::associated_token::ID, create_program_account()));

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn log_metadata_happy_path() {
    let result = run_log_metadata(TestParams::default());

    assert!(
        !result.program_result.is_err(),
        "log_metadata failed: {:?}",
        result.program_result
    );
}

#[test]
fn log_metadata_bridged_token_rejected() {
    // Bridged tokens have authority PDA as mint_authority → constraint fails
    let result = run_log_metadata(TestParams {
        bridged_token: true,
        ..Default::default()
    });

    assert!(
        result.program_result.is_err(),
        "should reject bridged token"
    );
}

#[test]
fn log_metadata_no_metadata_provided() {
    // metadata = None → TokenMetadataNotProvided (6004)
    let result = run_log_metadata(TestParams {
        provide_metadata: false,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6004))
    );
}

#[test]
fn log_metadata_wrong_metadata_address() {
    // metadata at wrong PDA → InvalidTokenMetadataAddress (6005)
    let result = run_log_metadata(TestParams {
        wrong_metadata_address: true,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6005))
    );
}
