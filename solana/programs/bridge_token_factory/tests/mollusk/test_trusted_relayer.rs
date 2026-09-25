use anchor_lang::{AnchorDeserialize, Space};
use bridge_token_factory::state::relayer::{RelayerEntry, RelayerList, RelayerState};
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

// Rent for one more entry in the relayer list
fn relayer_list_entry_rent(len: usize) -> u64 {
    let rent = Rent::default();
    rent.minimum_balance(RelayerList::space(len + 1))
        - rent.minimum_balance(RelayerList::space(len))
}

fn entry(relayer: Pubkey, stake: u64, activate_at: i64) -> RelayerEntry {
    RelayerEntry {
        relayer,
        stake,
        activate_at,
    }
}

fn list_contents(account: &Account) -> Vec<(Pubkey, u64, i64)> {
    entries_contents(&deserialize_relayer_list(&account.data).relayers)
}

fn entries_contents(entries: &[RelayerEntry]) -> Vec<(Pubkey, u64, i64)> {
    entries
        .iter()
        .map(|entry| (entry.relayer, entry.stake, entry.activate_at))
        .collect()
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
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("apply_for_trusted_relayer"),
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(relayer_list_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn build_resign_ix(program_id: &Pubkey, signer: &Pubkey) -> Instruction {
    let (relayer_state_pda, _) = find_relayer_pda(program_id, signer);
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("resign_trusted_relayer"),
        vec![
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(relayer_list_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
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
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    let mut data = anchor_ix_discriminator("grant_trusted_relayer").to_vec();
    data.extend_from_slice(relayer.as_ref());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(relayer_list_pda, false),
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
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    let mut data = anchor_ix_discriminator("reject_relayer_application").to_vec();
    data.extend_from_slice(relayer.as_ref());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_state_pda, false),
            AccountMeta::new(relayer_list_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
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

fn build_init_relayer_list_ix(
    program_id: &Pubkey,
    config_pda: &Pubkey,
    signer: &Pubkey,
) -> Instruction {
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    Instruction::new_with_bytes(
        *program_id,
        &anchor_ix_discriminator("init_relayer_list"),
        vec![
            AccountMeta::new_readonly(*config_pda, false),
            AccountMeta::new(relayer_list_pda, false),
            AccountMeta::new(*signer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

fn build_get_relayers_ix(
    program_id: &Pubkey,
    name: &str,
    from_index: u32,
    limit: u32,
) -> Instruction {
    let (relayer_list_pda, _) = find_relayer_list_pda(program_id);
    let mut data = anchor_ix_discriminator(name).to_vec();
    data.extend_from_slice(&from_index.to_le_bytes());
    data.extend_from_slice(&limit.to_le_bytes());
    Instruction::new_with_bytes(
        *program_id,
        &data,
        vec![AccountMeta::new_readonly(relayer_list_pda, false)],
    )
}

fn get_relayers(
    mollusk: &Mollusk,
    program_id: &Pubkey,
    list: &(Pubkey, Account),
    name: &str,
    from_index: u32,
    limit: u32,
) -> Vec<(Pubkey, u64, i64)> {
    let result = mollusk.process_instruction(
        &build_get_relayers_ix(program_id, name, from_index, limit),
        &[list.clone()],
    );
    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let entries = Vec::<RelayerEntry>::deserialize(&mut &result.return_data[..]).unwrap();
    entries_contents(&entries)
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
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(&program_id, vec![]);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer_list_pda, relayer_list_account),
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
        list_contents(result.get_account(&relayer_list_pda).unwrap()),
        vec![(relayer, STAKE, NOW + WAITING_PERIOD)]
    );

    assert_eq!(
        result.get_account(&relayer).unwrap().lamports,
        SIGNER_BALANCE - STAKE - relayer_state_rent() - relayer_list_entry_rent(0)
    );
}

#[test]
fn apply_for_trusted_relayer_staking_disabled() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), 0);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(&program_id, vec![]);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer_list_pda, relayer_list_account),
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
fn apply_for_trusted_relayer_before_relayer_config_rejected() {
    // AccountNotInitialized: the relayer list is created by init_relayer_list
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);
    let (relayer_list_pda, _) = find_relayer_list_pda(&program_id);

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer_list_pda, empty_account()),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(3012))
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
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(
        &program_id,
        vec![entry(relayer, STAKE, NOW + WAITING_PERIOD)],
    );

    let result = mollusk.process_instruction(
        &build_apply_ix(&program_id, &config_pda, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (relayer_list_pda, relayer_list_account),
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
    let other = Pubkey::new_unique();
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW);
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(
        &program_id,
        vec![entry(relayer, STAKE, NOW), entry(other, STAKE, NOW)],
    );

    let result = mollusk.process_instruction(
        &build_resign_ix(&program_id, &relayer),
        &[
            (relayer_state_pda, relayer_state_account),
            (relayer_list_pda, relayer_list_account),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    assert_eq!(result.get_account(&relayer_state_pda).unwrap().lamports, 0);
    assert_eq!(
        list_contents(result.get_account(&relayer_list_pda).unwrap()),
        vec![(other, STAKE, NOW)]
    );
    assert_eq!(
        result.get_account(&relayer).unwrap().lamports,
        SIGNER_BALANCE + STAKE + relayer_state_rent() + relayer_list_entry_rent(1)
    );
}

#[test]
fn resign_trusted_relayer_pending_rejected() {
    let (mollusk, program_id) = setup();
    let relayer = Pubkey::new_unique();
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + 1);
    let (relayer_list_pda, relayer_list_account) =
        create_relayer_list_account(&program_id, vec![entry(relayer, STAKE, NOW + 1)]);

    let result = mollusk.process_instruction(
        &build_resign_ix(&program_id, &relayer),
        &[
            (relayer_state_pda, relayer_state_account),
            (relayer_list_pda, relayer_list_account),
            (relayer, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
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
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(&program_id, vec![]);

    let result = mollusk.process_instruction(
        &build_grant_ix(&program_id, &config_pda, &admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer_list_pda, relayer_list_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);

    let state = deserialize_relayer_state(&result.get_account(&relayer_state_pda).unwrap().data);
    assert_eq!(state.stake, 0);
    assert_eq!(state.activate_at, NOW);
    assert_eq!(
        list_contents(result.get_account(&relayer_list_pda).unwrap()),
        vec![(relayer, 0, NOW)]
    );
}

#[test]
fn grant_trusted_relayer_by_non_admin_rejected() {
    let (mollusk, program_id) = setup();
    let non_admin = Pubkey::new_unique();
    let relayer = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), STAKE);
    let (relayer_state_pda, _) = find_relayer_pda(&program_id, &relayer);
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(&program_id, vec![]);

    let result = mollusk.process_instruction(
        &build_grant_ix(&program_id, &config_pda, &non_admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, empty_account()),
            (relayer_list_pda, relayer_list_account),
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
    let other = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, STAKE);
    let (relayer_state_pda, relayer_state_account) =
        create_relayer_state_account(&program_id, &relayer, STAKE, NOW + WAITING_PERIOD);
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(
        &program_id,
        vec![
            entry(other, STAKE, NOW),
            entry(relayer, STAKE, NOW + WAITING_PERIOD),
        ],
    );

    let result = mollusk.process_instruction(
        &build_reject_ix(&program_id, &config_pda, &admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (relayer_list_pda, relayer_list_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    assert_eq!(result.get_account(&relayer_state_pda).unwrap().lamports, 0);
    assert_eq!(
        list_contents(result.get_account(&relayer_list_pda).unwrap()),
        vec![(other, STAKE, NOW)]
    );
    assert_eq!(
        result.get_account(&admin).unwrap().lamports,
        SIGNER_BALANCE + STAKE + relayer_state_rent() + relayer_list_entry_rent(1)
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
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(
        &program_id,
        vec![entry(relayer, STAKE, NOW + WAITING_PERIOD)],
    );

    let result = mollusk.process_instruction(
        &build_reject_ix(&program_id, &config_pda, &non_admin, &relayer),
        &[
            (config_pda, config_account),
            (relayer_state_pda, relayer_state_account),
            (relayer_list_pda, relayer_list_account),
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

#[test]
fn init_relayer_list_by_admin() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, 0);
    let (relayer_list_pda, relayer_list_bump) = find_relayer_list_pda(&program_id);

    let result = mollusk.process_instruction(
        &build_init_relayer_list_ix(&program_id, &config_pda, &admin),
        &[
            (config_pda, config_account),
            (relayer_list_pda, empty_account()),
            (admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(!result.program_result.is_err(), "{:?}", result.program_result);
    let list = deserialize_relayer_list(&result.get_account(&relayer_list_pda).unwrap().data);
    assert_eq!(list.bump, relayer_list_bump);
    assert!(list.relayers.is_empty());
}

#[test]
fn init_relayer_list_twice_rejected() {
    let (mollusk, program_id) = setup();
    let admin = Pubkey::new_unique();
    let (config_pda, config_account) = config_with_relayer_params(&program_id, admin, 0);
    let (relayer_list_pda, relayer_list_account) = create_relayer_list_account(&program_id, vec![]);

    let result = mollusk.process_instruction(
        &build_init_relayer_list_ix(&program_id, &config_pda, &admin),
        &[
            (config_pda, config_account),
            (relayer_list_pda, relayer_list_account),
            (admin, create_signer_account(SIGNER_BALANCE)),
            (system_program::ID, create_native_program_account()),
        ],
    );

    assert!(result.program_result.is_err());
}

#[test]
fn init_relayer_list_by_non_admin_rejected() {
    let (mollusk, program_id) = setup();
    let non_admin = Pubkey::new_unique();
    let (config_pda, config_account) =
        config_with_relayer_params(&program_id, Pubkey::new_unique(), 0);
    let (relayer_list_pda, _) = find_relayer_list_pda(&program_id);

    let result = mollusk.process_instruction(
        &build_init_relayer_list_ix(&program_id, &config_pda, &non_admin),
        &[
            (config_pda, config_account),
            (relayer_list_pda, empty_account()),
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
fn get_pending_and_active_relayers() {
    let (mollusk, program_id) = setup();
    let (first, second, third) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
    let list = create_relayer_list_account(
        &program_id,
        vec![
            entry(first, STAKE, NOW - 1),
            entry(second, STAKE, NOW + 100),
            entry(third, 0, NOW),
        ],
    );

    assert_eq!(
        get_relayers(&mollusk, &program_id, &list, "get_active_relayers", 0, 10),
        vec![(first, STAKE, NOW - 1), (third, 0, NOW)]
    );
    assert_eq!(
        get_relayers(&mollusk, &program_id, &list, "get_pending_relayers", 0, 10),
        vec![(second, STAKE, NOW + 100)]
    );
}

#[test]
fn get_relayers_paginates() {
    let (mollusk, program_id) = setup();
    let relayers: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
    let list = create_relayer_list_account(
        &program_id,
        relayers
            .iter()
            .map(|relayer| entry(*relayer, STAKE, NOW + WAITING_PERIOD))
            .collect(),
    );
    let pending = |from_index, limit| {
        get_relayers(&mollusk, &program_id, &list, "get_pending_relayers", from_index, limit)
            .into_iter()
            .map(|(relayer, _, _)| relayer)
            .collect::<Vec<_>>()
    };

    assert_eq!(pending(1, 1), vec![relayers[1]]);
    assert_eq!(pending(1, 100), vec![relayers[1], relayers[2]]);
    assert!(pending(3, 100).is_empty());
    assert!(pending(0, 0).is_empty());
}

#[test]
fn get_relayers_page_is_capped() {
    let (mollusk, program_id) = setup();
    let list = create_relayer_list_account(
        &program_id,
        (0..25)
            .map(|_| entry(Pubkey::new_unique(), STAKE, NOW))
            .collect(),
    );

    let active = get_relayers(&mollusk, &program_id, &list, "get_active_relayers", 0, 100);
    assert_eq!(active.len(), 20);
}
