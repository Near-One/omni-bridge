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
const DECIMALS: u8 = 6;

struct TestParams {
    nonce: u64,
    amount: u128,
    paused: u8,
    max_used_nonce: u64,
    nonce_preset: bool,
    config_pubkey: Option<[u8; 64]>,
    malleable: bool,
    /// true = native token (vault exists), false = bridged token (no vault, mint_to)
    native_token: bool,
    /// For bridged token: use wrong mint authority (not authority PDA)
    wrong_mint_authority: bool,
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
            native_token: true,
            wrong_mint_authority: false,
        }
    }
}

fn run_finalize_transfer(params: TestParams) -> mollusk_svm::result::InstructionResult {
    let (mollusk, program_id) = setup_mollusk();
    let (secret, pubkey_bytes) = generate_bridge_keypair();

    let payer = Pubkey::new_unique();
    let recipient = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let token_program = anchor_spl::token::ID;

    let config_pubkey = params.config_pubkey.unwrap_or(pubkey_bytes);

    let (config_pda, config_account) = create_config_account(
        &program_id,
        &ConfigParams {
            paused: params.paused,
            max_used_nonce: params.max_used_nonce,
            derived_near_bridge_address: config_pubkey,
            ..Default::default()
        },
    );

    let (used_nonces_pda, used_nonces_account) = if params.nonce_preset {
        create_used_nonces_account_with_nonce_set(&program_id, params.nonce)
    } else {
        create_used_nonces_account(&program_id, params.nonce)
    };

    let (authority_pda, _) = find_authority_pda(&program_id);
    let authority_account = Account::new(10_000_000_000, 0, &system_program::ID);

    let recipient_account = Account::new(0, 0, &system_program::ID);

    // Create mint: for bridged tokens, mint_authority = authority PDA
    let mint_authority = if !params.native_token && !params.wrong_mint_authority {
        authority_pda
    } else {
        Pubkey::new_unique()
    };
    let mint_account = create_mint_account(Some(&mint_authority), 1_000_000_000, DECIMALS);

    // Vault: Some for native tokens, program_id sentinel for None (bridged)
    let (vault_key, vault_account) = if params.native_token {
        let (vault_pda, _) = find_vault_pda(&program_id, &mint);
        (vault_pda, create_token_account(&mint, &authority_pda, 1_000_000_000))
    } else {
        (program_id, create_program_account())
    };

    let (ata_pda, _) = find_associated_token_address(&recipient, &mint, &token_program);

    let payer_account = create_signer_account(10_000_000_000);

    let (wormhole_accounts, wormhole_metas) =
        build_wormhole_cpi_accounts(&config_pda, &config_account, &payer, &payer_account);

    let payload = FinalizeTransferPayload {
        destination_nonce: params.nonce,
        transfer_id: TransferId {
            origin_chain: ORIGIN_CHAIN,
            origin_nonce: ORIGIN_NONCE,
        },
        amount: params.amount,
        fee_recipient: None,
    };
    let serialized = payload.serialize_for_near((mint, recipient)).unwrap();
    let mut signature = sign_payload(&secret, &serialized);

    if params.malleable {
        signature = make_malleable_signature(&signature);
    }

    let mut ix_data = anchor_ix_discriminator("finalize_transfer").to_vec();
    anchor_lang::AnchorSerialize::serialize(&payload, &mut ix_data).unwrap();
    ix_data.extend_from_slice(&signature);

    let mut metas = vec![
        AccountMeta::new(config_pda, false),
        AccountMeta::new(used_nonces_pda, false),
        AccountMeta::new(authority_pda, false),
        AccountMeta::new_readonly(recipient, false),
        AccountMeta::new(mint, false),
        AccountMeta::new(vault_key, false),
        AccountMeta::new(ata_pda, false),
    ];
    metas.extend(wormhole_metas);
    metas.push(AccountMeta::new_readonly(anchor_spl::associated_token::ID, false));
    metas.push(AccountMeta::new_readonly(system_program::ID, false));
    metas.push(AccountMeta::new_readonly(token_program, false));

    let ix = Instruction::new_with_bytes(program_id, &ix_data, metas);

    let mut accounts = vec![
        (config_pda, config_account.clone()),
        (used_nonces_pda, used_nonces_account),
        (authority_pda, authority_account),
        (recipient, recipient_account),
        (mint, mint_account),
        (vault_key, vault_account),
        (ata_pda, Account::new(0, 0, &system_program::ID)),
    ];
    accounts.extend(wormhole_accounts);
    accounts.push(mollusk_svm_programs_token::associated_token::keyed_account());
    accounts.push((system_program::ID, create_native_program_account()));
    accounts.push((token_program, create_program_account()));

    mollusk.process_instruction(&ix, &accounts)
}

#[test]
fn finalize_transfer_native_happy_path() {
    let result = run_finalize_transfer(TestParams {
        native_token: true,
        ..Default::default()
    });

    assert!(
        !result.program_result.is_err(),
        "finalize_transfer (native) failed: {:?}",
        result.program_result
    );

    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.max_used_nonce, 1);
}

#[test]
fn finalize_transfer_bridged_happy_path() {
    let result = run_finalize_transfer(TestParams {
        native_token: false,
        ..Default::default()
    });

    assert!(
        !result.program_result.is_err(),
        "finalize_transfer (bridged) failed: {:?}",
        result.program_result
    );

    let config = deserialize_config(&result.resulting_accounts[0].1.data);
    assert_eq!(config.max_used_nonce, 1);
}

#[test]
fn finalize_transfer_paused() {
    let result = run_finalize_transfer(TestParams {
        paused: FINALIZE_TRANSFER_PAUSED,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6008))
    );
}

#[test]
fn finalize_transfer_bad_signature() {
    let result = run_finalize_transfer(TestParams {
        config_pubkey: Some([0u8; 64]),
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6001))
    );
}

#[test]
fn finalize_transfer_malleable_signature() {
    let result = run_finalize_transfer(TestParams {
        malleable: true,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6002))
    );
}

#[test]
fn finalize_transfer_nonce_reuse() {
    let result = run_finalize_transfer(TestParams {
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
fn finalize_transfer_invalid_bridged_token() {
    let result = run_finalize_transfer(TestParams {
        native_token: false,
        wrong_mint_authority: true,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6006))
    );
}

#[test]
fn finalize_transfer_amount_overflow() {
    let result = run_finalize_transfer(TestParams {
        amount: u128::from(u64::MAX) + 1,
        ..Default::default()
    });

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::Custom(6010))
    );
}
