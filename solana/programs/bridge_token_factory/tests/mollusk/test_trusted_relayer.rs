use anchor_lang::Space;
use bridge_token_factory::state::relayer::RelayerState;
use mollusk_svm::{result::ProgramResult, Mollusk};
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
};
use solana_sdk_ids::system_program;

use crate::mollusk::helpers::*;

const NOW: i64 = 1_700_000_000;
const STAKE: u64 = 5_000_000_000;
const WAITING_PERIOD: i64 = 7 * 24 * 60 * 60;
const SIGNER_BALANCE: u64 = 10_000_000_000;

fn setup() -> (Mollusk, Pubkey) {
    let (mut mollusk, program_id) = setup_mollusk();
    mollusk.sysvars.clock.unix_timestamp = NOW;
    (mollusk, program_id)
}

fn relayer_state_rent() -> u64 {
    Rent::default().minimum_balance(8 + RelayerState::INIT_SPACE)
}

fn config_with_relayer_params(
    program_id: &Pubkey,
    admin: Pubkey,
    stake_required: u64,
) -> (Pubkey, Account) {
    create_config_account(
        program_id,
        &ConfigParams {
            admin,
            relayer_stake_required: stake_required,
            relayer_waiting_period: WAITING_PERIOD,
            ..Default::default()
        },
    )
}

fn build_apply_ix(program_id: &Pubkey, config_pda: &Pubkey, signer: &Pubkey) -> Instruction {
    let (relayer_state_pda, _) = find_relayer_pda(program_id, signer);
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("apply_for_trusted_relayer"),
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn build_resign_ix(program_id: &Pubkey, signer: &Pubkey) -> Instruction {
    let (relayer_state_pda, _) = find_relayer_pda(program_id, signer);
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("resign_trusted_relayer"),
        vec![
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

fn build_grant_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    relayer: &Pubkey,
) -> Instruction {
    let (relayer_state_pda, _) = find_relayer_pda(program_id, relayer);
    let mut data = anchor_ix_discriminator("grant_trusted_relayer").to_vec();
    data.extend_from_slice(relayer.as_ref());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn build_reject_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    relayer: &Pubkey,
) -> Instruction {
    let (relayer_state_pda, _) = find_relayer_pda(program_id, relayer);
    let mut data = anchor_ix_discriminator("reject_relayer_application").to_vec();
    data.extend_from_slice(relayer.as_ref());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

fn build_set_relayer_config_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
    stake_required: u64,
    waiting_period: i64,
) -> Instruction {
    let mut data = anchor_ix_discriminator("set_relayer_config").to_vec();
    data.extend_from_slice(&stake_required.to_le_bytes());
    data.extend_from_slice(&waiting_period.to_le_bytes());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new(*config_pda, false),
            AccountMeta::new(*signer, true),
        ],
    )
}

fn empty_account() -> Account {
    Account::new(0, 0, &system_program::ID)
}

#[test]
fn apply_for_trusted_relayer_happy_path() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);

    let relayer_state_account = result.get_account(&relayer_state_pda).unwrap();
    assert_eq!(relayer_state_account.owner, program_id);
    assert_eq!(relayer_state_account.lamports, relayer_state_rent() + STAKE);

    let state = deserialize_relayer_state(&relayer_state_account.data);
    assert_eq!(state.stake, STAKE);
    assert_eq!(state.activate_at, NOW + WAITING_PERIOD);
    assert_eq!(state.bump, find_relayer_pda(&program_id, &relayer).1);

    assert_eq!(
        result.get_account(&relayer).unwrap().lamports,
        SIGNER_BALANCE - STAKE - relayer_state_rent()
    );
}

#[test]
fn apply_for_trusted_relayer_staking_disabled() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), 0);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6011))
    );
}

#[test]
fn apply_for_trusted_relayer_twice_rejected() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + WAITING_PERIOD);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(result.program_result.is_err());
}

#[test]
fn resign_trusted_relayer_returns_stake() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW);

    let result = mollusk.process_instruction(
        &build_resign_ix(&program_id, &relayer),
        &[
            (relayer_state_pda, relayer_state_account),
            (relayer, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    assert_eq!(result.get_account(&relayer_state_pda).unwrap().lamports, 0);
    assert_eq!(
        result.get_account(&relayer).unwrap().lamports,
        SIGNER_BALANCE + STAKE + relayer_state_rent()
    );
}

#[test]
fn resign_trusted_relayer_pending_rejected() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + 1);

    let result = mollusk.process_instruction(
        &build_resign_ix(&program_id, &relayer),
        &[
            (relayer_state_pda, relayer_state_account),
            (relayer, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6012))
    );
}

#[test]
fn grant_trusted_relayer_by_admin() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, STAKE);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);

    let result = mollusk.process_instruction(
        &build_grant_ix(&program_id, &config_pda, &admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);

    let state = deserialize_relayer_state(&result.get_account(&relayer_state_pda).unwrap().data);
    assert_eq!(state.stake, 0);
    assert_eq!(state.activate_at, NOW);
}

#[test]
fn grant_trusted_relayer_by_non_admin_rejected() {
    let (mollusk, program_id) = setup();
    let non_admin = Pubkey::new_unique();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);

    let result = mollusk.process_instruction(
        &build_grant_ix(&program_id, &config_pda, &non_admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (non_admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn reject_relayer_application_takes_stake() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, STAKE);
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + WAITING_PERIOD);

    let result = mollusk.process_instruction(
        &build_reject_ix(&program_id, &config_pda, &admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    assert_eq!(result.get_account(&relayer_state_pda).unwrap().lamports, 0);
    assert_eq!(
        result.get_account(&admin).unwrap().lamports,
        SIGNER_BALANCE + STAKE + relayer_state_rent()
    );
}

#[test]
fn reject_relayer_application_by_non_admin_rejected() {
    let (mollusk, program_id) = setup();
    let non_admin = Pubkey::new_unique();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + WAITING_PERIOD);

    let result = mollusk.process_instruction(
        &build_reject_ix(&program_id, &config_pda, &non_admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (non_admin, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn set_relayer_config_by_admin() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, 0);

    let result = mollusk.process_instruction(
        &build_set_relayer_config_ix(&program_id, &config_pda, &admin, STAKE, 3600),
        &[
            (config_pda, config_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let config = deserialize_config(&result.get_account(&config_pda).unwrap().data);
    assert_eq!(config.relayer_stake_required, STAKE);
    assert_eq!(config.relayer_waiting_period, 3600);
}

#[test]
fn set_relayer_config_negative_waiting_period_rejected() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, 0);

    let result = mollusk.process_instruction(
        &build_set_relayer_config_ix(&program_id, &config_pda, &admin, STAKE, -1),
        &[
            (config_pda, config_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6000))
    );
}

#[test]
fn set_relayer_config_by_non_admin_rejected() {
    let (mollusk, program_id) = setup();
    let non_admin = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), 0);

    let result = mollusk.process_instruction(
        &build_set_relayer_config_ix(&program_id, &config_pda, &non_admin, STAKE, 3600),
        &[
            (config_pda, config_account),
            (non_admin, create_signer_account(SIGNER_BALANCE)),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}
