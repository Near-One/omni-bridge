use bridge_token_factory::state::message::{
    finalize_transfer::FinalizeTransferPayload, Payload, TransferId,
};
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
const ORIGIN_CHAIN: u8 = 1;
const ORIGIN_NONCE: u64 = 42;

struct TestParams {
    nonce: u64,
    amount: u128,
    paused: u8,
    max_used_nonce: u64,
    /// If true, pre-mark the nonce as used in the UsedNonces account
    nonce_preset: bool,
    /// If Some, override the derived_near_bridge_address in config
    config_pubkey: Option<[u8; 64]>,
    /// If true, make the signature malleable (high-s)
    malleable: bool,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            nonce: 1,
            amount: TRANSFER_AMOUNT,
            paused: 0,
            max_used_nonce: 0,
            nonce_preset: false,
            config_pubkey: None,
            malleable: false,
        }
    }
}

fn run_finalize_transfer_sol(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();
    let (secret, pubkey_bytes) = generate_bridge_keypair();

    let admin = Pubkey::new_unique();
    let recipient = Pubkey::new_unique();
    let payer = Pubkey::new_unique();

    let config_pubkey = params.config_pubkey.unwrap_or(pubkey_bytes);

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            admin,
            paused: params.paused,
            max_used_nonce: params.max_used_nonce,
            derived_near_bridge_address: config_pubkey,
            ..Default::default()
        },
    );

    // UsedNonces: pre-initialized, optionally with nonce already marked
    let (used_nonces_pda, used_nonces_account) = if params.nonce_preset {
        create_used_nonces_account_with_nonce_set(&program_id, params.nonce)
    } else {
        create_used_nonces_account(&program_id, params.nonce)
    };

    let (authority_pda, _) = find_authority_pda(&program_id);
    let authority_account = Account::new(10_000_000_000, 0, &system_program::ID);

    let recipient_account = Account::new(0, 0, &system_program::ID);

    let (sol_vault_pda, _) = find_sol_vault_pda(&program_id);
    let sol_vault_account = Account::new(10_000_000_000, 0, &system_program::ID);

    let payer_account = create_signer_account(10_000_000_000);

    let (wormhole_accounts, wormhole_metas) =
        build_wormhole_cpi_accounts(&config_pda, &config_account, &payer, &payer_account);

    let payload_for_sign = FinalizeTransferPayload {
        destination_nonce: params.nonce,
        transfer_id: TransferId {
            origin_chain: ORIGIN_CHAIN,
            origin_nonce: ORIGIN_NONCE,
        },
        amount: params.amount,
        fee_recipient: None,
    };
    let serialized = payload_for_sign
        .serialize_for_near((Pubkey::default(), recipient))
        .unwrap();
    let mut signature = sign_payload(&secret, &serialized);

    if params.malleable {
        signature = make_malleable_signature(&signature);
    }

    let payload_for_ix = FinalizeTransferPayload {
        destination_nonce: params.nonce,
        transfer_id: TransferId {
            origin_chain: ORIGIN_CHAIN,
            origin_nonce: ORIGIN_NONCE,
        },
        amount: params.amount,
        fee_recipient: None,
    };
    let mut ix_data = anchor_ix_discriminator("finalize_transfer_sol").to_vec();
    anchor_lang::AnchorSerialize::serialize(&payload_for_ix, &mut ix_data).unwrap();
    ix_data.extend_from_slice(&signature);

    let mut metas = vec![
        AccountMeta::new(config_pda, false),
        AccountMeta::new(used_nonces_pda, false),
        AccountMeta::new(authority_pda, false),
        AccountMeta::new(recipient, false),
        AccountMeta::new(sol_vault_pda, false),
    ];
    metas.extend(wormhole_metas);
    metas.push(AccountMeta::new_readonly(system_program::ID, false));

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let mut accounts = vec![
        (config_pda, config_account.clone()),
        (used_nonces_pda, used_nonces_account),
        (authority_pda, authority_account),
        (recipient, recipient_account),
        (sol_vault_pda, sol_vault_account),
    ];
    accounts.extend(wormhole_accounts);
    accounts.push((system_program::ID, create_native_program_account()));

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn finalize_transfer_sol_happy_path() {
    let result = run_finalize_transfer_sol(TestParams::default());

    assert!(
        !result.program_result.is_err(),
        "finalize_transfer_sol failed: {:?}",
        result.program_result
    );

    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.max_used_nonce, 1);
}

#[test]
fn finalize_transfer_sol_paused() {
    let result = run_finalize_transfer_sol(TestParams {
        paused: FINALIZE_TRANSFER_PAUSED,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6008))
    );
}

#[test]
fn finalize_transfer_sol_bad_signature() {
    // Config has wrong public key → signature verification fails
    let result = run_finalize_transfer_sol(TestParams {
        config_pubkey: Some([0u8; 64]),
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6001))
    );
}

#[test]
fn finalize_transfer_sol_malleable_signature() {
    let result = run_finalize_transfer_sol(TestParams {
        malleable: true,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6002))
    );
}

#[test]
fn finalize_transfer_sol_nonce_reuse() {
    let result = run_finalize_transfer_sol(TestParams {
        nonce: 5,
        nonce_preset: true,
        max_used_nonce: 5,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6003))
    );
}

#[test]
fn finalize_transfer_sol_amount_overflow() {
    // Amount > u64::MAX can't be converted for SOL transfer
    let result = run_finalize_transfer_sol(TestParams {
        amount: u128::from(u64::MAX) + 1,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6010))
    );
}
