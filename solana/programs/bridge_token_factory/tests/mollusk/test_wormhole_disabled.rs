use anchor_lang::prelude::AccountMeta;
use bridge_token_factory::state::message::init_transfer::InitTransferPayload;
use mollusk_svm::result::{InstructionResult, ProgramResult};
use solana_log_collector::LogCollector;
use solana_sdk::{
    account::Account,
    instruction::{Instruction, InstructionError},
    program_error::ProgramError,
    pubkey::Pubkey,
};
use solana_sdk_ids::system_program;

use crate::mollusk::helpers::*;

const SEQUENCE: u64 = 42;

struct TestParams {
    wormhole_disabled: bool,
    next_origin_nonce: u64,
    config_writable: bool,
    with_program_account: bool,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            wormhole_disabled: true,
            next_origin_nonce: 0,
            config_writable: true,
            with_program_account: true,
        }
    }
}

struct TestResult {
    result: InstructionResult,
    logs: Vec<String>,
    config_pda: Pubkey,
}

impl TestResult {
    fn next_origin_nonce(&self) -> u64 {
        let config = self.result.get_account(&self.config_pda).unwrap();
        deserialize_config(&config.data).next_origin_nonce
    }

    fn invoked(&self, program_id: &Pubkey) -> bool {
        let invoke = format!("Program {program_id} invoke [2]");
        self.logs.iter().any(|log| *log == invoke)
    }
}

fn run_init_transfer_sol(params: TestParams) -> TestResult {
    let (mut mollusk, program_id) = setup_mollusk();
    let logger = LogCollector::new_ref();
    mollusk.logger = Some(logger.clone());

    let user = Pubkey::new_unique();
    let user_account = create_signer_account(10_000_000_000);

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            wormhole_disabled: params.wormhole_disabled,
            next_origin_nonce: params.next_origin_nonce,
            ..Default::default()
        },
    );

    let (sol_vault_pda, _) = find_sol_vault_pda(&program_id);
    let sol_vault_account = Account::new(1_000_000_000, 0, &system_program::ID);

    let (mut wormhole_accounts, mut wormhole_metas) =
        build_wormhole_cpi_accounts(&config_pda, &config_account, &user, &user_account);
    let sequence = create_sequence_tracker_account(&wormhole_program_id(), &config_pda, SEQUENCE);
    for entry in &mut wormhole_accounts {
        if entry.0 == sequence.0 {
            *entry = sequence.clone();
        }
    }
    wormhole_metas[0].is_writable = params.config_writable;

    let payload = InitTransferPayload {
        amount: 1_000_000,
        recipient: "recipient.near".to_string(),
        fee: 0,
        native_fee: 100,
        message: String::new(),
    };
    let mut ix_data = anchor_ix_discriminator("init_transfer_sol").to_vec();
    anchor_lang::AnchorSerialize::serialize(&payload, &mut ix_data).unwrap();

    let mut metas = vec![
        AccountMeta::new(sol_vault_pda, false),
        AccountMeta::new(user, true),
    ];
    metas.extend(wormhole_metas);
    let mut accounts = vec![(sol_vault_pda, sol_vault_account), (user, user_account)];
    accounts.extend(wormhole_accounts);
    if params.with_program_account {
        // Mollusk stubs the account of the program under test itself.
        metas.push(AccountMeta::new_readonly(program_id, false));
    }

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);
    let result = mollusk.process_instruction(&ix, &accounts);
    let logs = logger.borrow().get_recorded_content().to_vec();

    TestResult {
        result,
        logs,
        config_pda,
    }
}

fn run_set_wormhole_disabled(signer_is_admin: bool, wormhole_disabled: bool) -> InstructionResult {
    let (mollusk, program_id) = setup_mollusk();
    let admin = Pubkey::new_unique();
    let signer = if signer_is_admin {
        admin
    } else {
        Pubkey::new_unique()
    };
    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            wormhole_disabled: !wormhole_disabled,
            ..Default::default()
        },
    );

    let mut data = anchor_ix_discriminator("set_wormhole_disabled").to_vec();
    data.push(u8::from(wormhole_disabled));
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![
            AccountMeta::new(config_pda, false),
            AccountMeta::new(signer, true),
        ],
    );

    mollusk.process_instruction(
        &ix,
        &[
            (config_pda, config_account),
            (signer, create_signer_account(1_000_000_000)),
        ],
    )
}

#[test]
fn set_wormhole_disabled_by_admin() {
    for wormhole_disabled in [true, false] {
        let result = run_set_wormhole_disabled(true, wormhole_disabled);

        assert!(
            !result.program_result.is_err(),
            "{:?}",
            result.program_result
        );
        let (config_pda, _) = find_config_pda(&bridge_program_id());
        let config = deserialize_config(&result.get_account(&config_pda).unwrap().data);
        assert_eq!(config.wormhole_disabled, wormhole_disabled);
    }
}

#[test]
fn set_wormhole_disabled_by_non_admin_rejected() {
    let result = run_set_wormhole_disabled(false, true);

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6009))
    );
}

#[test]
fn init_transfer_sol_wormhole_enabled_uses_wormhole_sequence() {
    let test = run_init_transfer_sol(TestParams {
        wormhole_disabled: false,
        config_writable: false,
        with_program_account: false,
        ..Default::default()
    });

    assert!(
        !test.result.program_result.is_err(),
        "{:?}",
        test.result.program_result
    );
    assert!(test.invoked(&wormhole_shim_id()));
    assert!(!test.invoked(&bridge_program_id()));
    assert_eq!(test.next_origin_nonce(), 0);
}

#[test]
fn init_transfer_sol_wormhole_disabled_emits_message_instead_of_wormhole() {
    let test = run_init_transfer_sol(TestParams::default());

    assert!(
        !test.result.program_result.is_err(),
        "{:?}",
        test.result.program_result
    );
    assert!(test.invoked(&bridge_program_id()));
    assert!(!test.invoked(&wormhole_shim_id()));
    assert!(test
        .logs
        .iter()
        .any(|log| log == "Program log: Instruction: EmitMessage"));
}

#[test]
fn init_transfer_sol_wormhole_disabled_continues_from_wormhole_sequence() {
    let test = run_init_transfer_sol(TestParams::default());

    assert!(
        !test.result.program_result.is_err(),
        "{:?}",
        test.result.program_result
    );
    assert_eq!(test.next_origin_nonce(), SEQUENCE + 1);
}

#[test]
fn init_transfer_sol_wormhole_disabled_increments_own_nonce() {
    let test = run_init_transfer_sol(TestParams {
        next_origin_nonce: 100,
        ..Default::default()
    });

    assert!(
        !test.result.program_result.is_err(),
        "{:?}",
        test.result.program_result
    );
    assert_eq!(test.next_origin_nonce(), 101);
}

#[test]
fn init_transfer_sol_wormhole_reenabled_keeps_own_nonce() {
    let test = run_init_transfer_sol(TestParams {
        wormhole_disabled: false,
        next_origin_nonce: 100,
        with_program_account: false,
        ..Default::default()
    });

    assert!(
        !test.result.program_result.is_err(),
        "{:?}",
        test.result.program_result
    );
    assert!(test.invoked(&wormhole_shim_id()));
    assert_eq!(test.next_origin_nonce(), 101);
}

#[test]
fn init_transfer_sol_wormhole_disabled_readonly_config_rejected() {
    let test = run_init_transfer_sol(TestParams {
        config_writable: false,
        ..Default::default()
    });

    // anchor_lang::error::ErrorCode::ConstraintMut
    assert_eq!(
        test.result.program_result,
        ProgramResult::Failure(ProgramError::Custom(2000))
    );
}

#[test]
fn init_transfer_sol_wormhole_disabled_without_program_account_rejected() {
    let test = run_init_transfer_sol(TestParams {
        with_program_account: false,
        ..Default::default()
    });

    assert_eq!(
        test.result.raw_result,
        Err(InstructionError::MissingAccount)
    );
}

#[test]
fn emit_message_without_config_signature_rejected() {
    let (mollusk, program_id) = setup_mollusk();
    let (config_pda, config_account) = create_config_account(&program_id, &ConfigParams::default());

    let mut data = anchor_ix_discriminator("emit_message").to_vec();
    anchor_lang::AnchorSerialize::serialize(&vec![0u8; 8], &mut data).unwrap();
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![AccountMeta::new_readonly(config_pda, false)],
    );

    let result = mollusk.process_instruction(&ix, &[(config_pda, config_account)]);

    // anchor_lang::error::ErrorCode::AccountNotSigner
    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(3010))
    );
}

#[test]
fn emit_message_signed_by_other_account_rejected() {
    let (mollusk, program_id) = setup_mollusk();
    let signer = Pubkey::new_unique();

    let mut data = anchor_ix_discriminator("emit_message").to_vec();
    anchor_lang::AnchorSerialize::serialize(&vec![0u8; 8], &mut data).unwrap();
    let ix = Instruction::new_with_bytes(
        program_id,
        &data,
        vec![AccountMeta::new_readonly(signer, true)],
    );

    let result =
        mollusk.process_instruction(&ix, &[(signer, create_signer_account(1_000_000_000))]);

    // anchor_lang::error::ErrorCode::ConstraintSeeds
    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(2006))
    );
}
