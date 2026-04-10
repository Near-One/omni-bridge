use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_sdk_ids::system_program;
use mollusk_svm::result::ProgramResult;

use crate::mollusk::helpers::*;

/// Marker written by the stub program to the last bytes of its first writable account.
/// Asserting this confirms the Metaplex CPI was actually invoked by the bridge program.
const STUB_INVOCATION_MARKER: [u8; 4] = stub_program::INVOCATION_MARKER;

const DECIMALS: u8 = 6;

enum Signer {
    Admin,
    MetadataAdmin,
    Unauthorized,
}

struct TestParams {
    signer_type: Signer,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            signer_type: Signer::Admin,
        }
    }
}

fn run_update_metadata(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();

    let admin = Pubkey::new_unique();
    let metadata_admin = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = anchor_spl::token::ID;

    let signer = match params.signer_type {
        Signer::Admin => admin,
        Signer::MetadataAdmin => metadata_admin,
        Signer::Unauthorized => Pubkey::new_unique(),
    };

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            metadata_admin,
            ..Default::default()
        },
    );

    let (authority_pda, _) = find_authority_pda(&program_id);
    let authority_account = Account::new(0, 0, &system_program::ID);

    let mint_account = create_mint_account(Some(&authority_pda), 1_000_000_000, DECIMALS);

    let (metadata_pda, _) = find_metaplex_metadata_pda(&mint);
    let metadata_account = create_metaplex_metadata_account(
        &authority_pda,
        &mint,
        "Test Token",
        "TEST",
    );

    let signer_account = create_signer_account(10_000_000_000);

    let mut ix_data = anchor_ix_discriminator("update_metadata").to_vec();
    ix_data.push(1);
    let name = "New Name";
    ix_data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    ix_data.extend_from_slice(name.as_bytes());
    ix_data.push(0);
    ix_data.push(0);

    let metas = vec![
        AccountMeta::new_readonly(config_pda, false),
        AccountMeta::new_readonly(authority_pda, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(metadata_pda, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(metaplex_id(), false),
        AccountMeta::new(signer, true),
    ];

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let accounts = vec![
        (config_pda, config_account),
        (authority_pda, authority_account),
        (mint, mint_account),
        (metadata_pda, metadata_account),
        (token_program, create_program_account()),
        (metaplex_id(), create_program_account()),
        (signer, signer_account),
    ];

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn update_metadata_by_admin() {
    let result = run_update_metadata(TestParams {
        signer_type: Signer::Admin,
    });

    assert!(
        !result.program_result.is_err(),
        "update_metadata by admin failed: {:?}",
        result.program_result
    );

    let metadata_data = &result.resulting_accounts[3].1.data;
    assert_eq!(&metadata_data[metadata_data.len() - 4..], STUB_INVOCATION_MARKER);
}

#[test]
fn update_metadata_by_metadata_admin() {
    let result = run_update_metadata(TestParams {
        signer_type: Signer::MetadataAdmin,
    });

    assert!(
        !result.program_result.is_err(),
        "update_metadata by metadata_admin failed: {:?}",
        result.program_result
    );

    let metadata_data = &result.resulting_accounts[3].1.data;
    assert_eq!(&metadata_data[metadata_data.len() - 4..], STUB_INVOCATION_MARKER);
}

#[test]
fn update_metadata_unauthorized() {
    let result = run_update_metadata(TestParams {
        signer_type: Signer::Unauthorized,
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}
