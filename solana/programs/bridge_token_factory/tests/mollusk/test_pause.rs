use anchor_lang::prelude::AccountMeta;
use mollusk_svm::result::ProgramResult;
use solana_sdk::{instruction::Instruction, program_error::ProgramError, pubkey::Pubkey};

use crate::mollusk::helpers::*;

pub fn build_pause_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("pause"),
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

#[test]
fn pause_by_admin() {
    let (mollusk, program_id) = setup_mollusk();

    let admin = Pubkey::new_unique();
    let pausable_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            pausable_admin,
            paused: 0,
            ..Default::default()
        },
    );

    let ix = build_pause_ix(&program_id, &config_pda, &admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(
        !result.program_result.is_err(),
        "pause_by_admin failed: {:?}",
        result.program_result
    );

    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.paused, ALL_PAUSED);
}

#[test]
fn pause_by_pausable_admin() {
    let (mollusk, program_id) = setup_mollusk();

    let admin = Pubkey::new_unique();
    let pausable_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            pausable_admin,
            paused: 0,
            ..Default::default()
        },
    );

    let ix = build_pause_ix(&program_id, &config_pda, &pausable_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[
            (config_pda, config_account),
            (pausable_admin, create_signer_account(1_000_000_000)),
        ],
    );

    assert!(
        !result.program_result.is_err(),
        "pause_by_pausable_admin failed: {:?}",
        result.program_result
    );

    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.paused, ALL_PAUSED);
}

#[test]
fn pause_unauthorized() {
    let (mollusk, program_id) = setup_mollusk();

    let admin = Pubkey::new_unique();
    let pausable_admin = Pubkey::new_unique();
    let random_signer = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            pausable_admin,
            paused: 0,
            ..Default::default()
        },
    );

    let ix = build_pause_ix(&program_id, &config_pda, &random_signer);
    let result = mollusk.process_instruction(
        &ix,
        &[
            (config_pda, config_account),
            (random_signer, create_signer_account(1_000_000_000)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}
