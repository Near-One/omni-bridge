use anchor_lang::prelude::AccountMeta;
use mollusk_svm::result::ProgramResult;
use solana_sdk::{instruction::Instruction, program_error::ProgramError, pubkey::Pubkey};

use crate::mollusk::helpers::*;

pub fn build_unpause_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    paused: u8,
) -> Instruction {
    let mut data = anchor_ix_discriminator("unpause").to_vec();
    data.push(paused);
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

pub fn build_set_admin_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    new_admin: &Pubkey,
) -> Instruction {
    let mut data = anchor_ix_discriminator("set_admin").to_vec();
    data.extend_from_slice(&new_admin.to_bytes());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

pub fn build_set_pausable_admin_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    new_pausable_admin: &Pubkey,
) -> Instruction {
    let mut data = anchor_ix_discriminator("set_pausable_admin").to_vec();
    data.extend_from_slice(&new_pausable_admin.to_bytes());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

pub fn build_set_metadata_admin_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    new_metadata_admin: &Pubkey,
) -> Instruction {
    let mut data = anchor_ix_discriminator("set_metadata_admin").to_vec();
    data.extend_from_slice(&new_metadata_admin.to_bytes());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

#[test]
fn unpause_all() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            paused: ALL_PAUSED,
            ..Default::default()
        },
    );

    let ix = build_unpause_ix(&program_id, &config_pda, &admin, 0);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.paused, 0);
}

#[test]
fn unpause_partial() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            paused: ALL_PAUSED,
            ..Default::default()
        },
    );

    let ix = build_unpause_ix(&program_id, &config_pda, &admin, 2);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.paused, 2);
}

#[test]
fn unpause_by_pausable_admin_rejected() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let pausable_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            pausable_admin,
            paused: ALL_PAUSED,
            ..Default::default()
        },
    );

    let ix = build_unpause_ix(&program_id, &config_pda, &pausable_admin, 0);
    let result = mollusk.process_instruction(
        &ix,
        &[
            (config_pda, config_account),
            (pausable_admin, create_signer_account(1_000_000_000)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn unpause_unauthorized() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let random = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            paused: ALL_PAUSED,
            ..Default::default()
        },
    );

    let ix = build_unpause_ix(&program_id, &config_pda, &random, 0);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (random, create_signer_account(1_000_000_000))],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn set_admin_happy_path() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let new_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_admin_ix(&program_id, &config_pda, &admin, &new_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.admin, new_admin);
}

#[test]
fn set_admin_unauthorized() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let random = Pubkey::new_unique();
    let new_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_admin_ix(&program_id, &config_pda, &random, &new_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (random, create_signer_account(1_000_000_000))],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn set_pausable_admin_happy_path() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let new_pausable_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_pausable_admin_ix(&program_id, &config_pda, &admin, &new_pausable_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.pausable_admin, new_pausable_admin);
}

#[test]
fn set_pausable_admin_unauthorized() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let random = Pubkey::new_unique();
    let new_pausable_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_pausable_admin_ix(&program_id, &config_pda, &random, &new_pausable_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (random, create_signer_account(1_000_000_000))],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn set_metadata_admin_happy_path() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let new_metadata_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_metadata_admin_ix(&program_id, &config_pda, &admin, &new_metadata_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (admin, create_signer_account(1_000_000_000))],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.metadata_admin, new_metadata_admin);
}

#[test]
fn set_metadata_admin_unauthorized() {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let random = Pubkey::new_unique();
    let new_metadata_admin = Pubkey::new_unique();

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            ..Default::default()
        },
    );

    let ix = build_set_metadata_admin_ix(&program_id, &config_pda, &random, &new_metadata_admin);
    let result = mollusk.process_instruction(
        &ix,
        &[(config_pda, config_account), (random, create_signer_account(1_000_000_000))],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}
