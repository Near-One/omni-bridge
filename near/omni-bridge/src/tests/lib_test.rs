use std::collections::HashMap;
use std::str::FromStr;

use near_contract_standards::storage_management::StorageBalance;
use near_sdk::{
    borsh, json_types::U128, serde_json, test_utils::VMContextBuilder, test_vm_config, testing_env,
    AccountId, NearToken, PromiseOrValue, PromiseResult, RuntimeFeesConfig,
};
use omni_types::{
    locker_args::StorageDepositAction,
    prover_result::{InitTransferMessage, ProverResult},
    sol_address::SolAddress,
    BridgeOnTransferMsg, ChainKind, EvmAddress, Fee, InitTransferMsg, Nonce, OmniAddress,
    TransferId, TransferMessage, UpdateFee,
};

use crate::storage::Decimals;
use crate::Contract;

const DEFAULT_NONCE: Nonce = 0;
const DEFAULT_TRANSFER_ID: TransferId = TransferId {
    origin_chain: ChainKind::Near,
    origin_nonce: DEFAULT_NONCE,
};
const DEFAULT_MPC_SIGNER_ACCOUNT: &str = "mpc_signer.testnet";
const DEFAULT_WNEAR_ACCOUNT: &str = "wnear.testnet";

const DEFAULT_NEAR_USER_ACCOUNT: &str = "user.testnet";
const DEFAULT_FT_CONTRACT_ACCOUNT: &str = "ft_contract.testnet";
const DEFAULT_ETH_USER_ADDRESS: &str = "0x1234567890123456789012345678901234567890";
const DEFAULT_TRANSFER_AMOUNT: u128 = 100;
const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1_250_000_000_000_000_000_000);

fn setup_test_env(
    predecessor_account_id: AccountId,
    attached_deposit: NearToken,
    promise_results: Option<Vec<PromiseResult>>,
) {
    let context = VMContextBuilder::new()
        .predecessor_account_id(predecessor_account_id)
        .attached_deposit(attached_deposit)
        .signer_account_id(DEFAULT_NEAR_USER_ACCOUNT.parse().unwrap())
        .build();

    if let Some(results) = promise_results {
        testing_env!(
            context,
            test_vm_config(),
            RuntimeFeesConfig::test(),
            HashMap::default(),
            results,
        );
    } else {
        testing_env!(context);
    }
}

fn setup_contract(mpc_signer_id: String, wnear_id: String) -> Contract {
    Contract::new(
        AccountId::try_from(mpc_signer_id).expect("Invalid default mpc signer ID"),
        AccountId::try_from(wnear_id).expect("Invalid default wnear ID"),
    )
}

fn get_default_contract() -> Contract {
    setup_contract(
        DEFAULT_MPC_SIGNER_ACCOUNT.to_string(),
        DEFAULT_WNEAR_ACCOUNT.to_string(),
    )
}

fn run_storage_deposit(
    contract: &mut Contract,
    account_id: AccountId,
    storage_deposit_balance: NearToken,
) {
    setup_test_env(account_id.clone(), storage_deposit_balance, None);
    contract.storage_deposit(Some(account_id));
}

fn get_init_transfer_msg(recipient: &str, fee: u128, native_token_fee: u128) -> InitTransferMsg {
    InitTransferMsg {
        recipient: OmniAddress::Eth(EvmAddress::from_str(recipient).unwrap()),
        fee: U128(fee),
        native_token_fee: U128(native_token_fee),
        msg: None,
    }
}

fn run_ft_on_transfer(
    contract: &mut Contract,
    sender_id: String,
    token_id: String,
    amount: U128,
    attached_deposit: Option<NearToken>,
    msg: &BridgeOnTransferMsg,
) {
    let sender_id = AccountId::try_from(sender_id).expect("Invalid sender ID");
    let token_id = AccountId::try_from(token_id).expect("Invalid token ID");

    let attached_deposit = if let Some(deplosit) = attached_deposit {
        deplosit
    } else {
        let min_storage_balance = contract.required_balance_for_account();
        let init_transfer_balance = contract.required_balance_for_init_transfer(None);
        min_storage_balance.saturating_add(init_transfer_balance)
    };

    run_storage_deposit(contract, sender_id.clone(), attached_deposit);
    setup_test_env(token_id.clone(), NearToken::from_yoctonear(0), None);

    let msg = serde_json::to_string(msg).expect("Failed to serialize transfer message");

    contract.ft_on_transfer(sender_id, amount, msg);
}

fn run_ft_on_transfer_legacy(
    contract: &mut Contract,
    sender_id: String,
    token_id: String,
    amount: U128,
    attached_deposit: Option<NearToken>,
    msg: &InitTransferMsg,
) {
    let sender_id = AccountId::try_from(sender_id).expect("Invalid sender ID");
    let token_id = AccountId::try_from(token_id).expect("Invalid token ID");

    let attached_deposit = if let Some(deposit) = attached_deposit {
        deposit
    } else {
        let min_storage_balance = contract.required_balance_for_account();
        let init_transfer_balance = contract.required_balance_for_init_transfer(None);
        min_storage_balance.saturating_add(init_transfer_balance)
    };

    run_storage_deposit(contract, sender_id.clone(), attached_deposit);
    setup_test_env(token_id.clone(), NearToken::from_yoctonear(0), None);

    let msg = serde_json::to_string(msg).expect("Failed to serialize transfer message");

    contract.ft_on_transfer(sender_id, amount, msg);
}

#[test]
fn test_initialize_contract() {
    let contract = get_default_contract();

    assert_eq!(contract.mpc_signer, DEFAULT_MPC_SIGNER_ACCOUNT);
    assert_eq!(contract.current_origin_nonce, DEFAULT_NONCE);
    assert_eq!(contract.wnear_account_id, DEFAULT_WNEAR_ACCOUNT);
}

#[test]
fn test_init_transfer_nonce_increment() {
    let mut contract = get_default_contract();

    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(100),
        None,
        &BridgeOnTransferMsg::InitTransfer(get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS, 0, 0)),
    );

    assert_eq!(contract.current_origin_nonce, DEFAULT_NONCE + 1);
}

#[test]
fn test_init_transfer_stored_transfer_message() {
    let mut contract = get_default_contract();

    let msg = get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS, 0, 0);
    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        &BridgeOnTransferMsg::InitTransfer(msg.clone()),
    );

    let stored_transfer = contract.get_transfer_message(TransferId {
        origin_chain: ChainKind::Near,
        origin_nonce: contract.current_origin_nonce,
    });
    assert_eq!(
        stored_transfer.recipient, msg.recipient,
        "Incorrect stored recipient"
    );
    assert_eq!(stored_transfer.fee.fee, msg.fee, "Incorrect stored fee");
    assert_eq!(
        stored_transfer.fee.native_fee, msg.native_token_fee,
        "Incorrect stored native fee"
    );
    assert_eq!(
        stored_transfer.sender,
        OmniAddress::Near(DEFAULT_NEAR_USER_ACCOUNT.parse().unwrap()),
        "Incorrect stored sender"
    );
    assert_eq!(
        stored_transfer.token,
        OmniAddress::Near(DEFAULT_FT_CONTRACT_ACCOUNT.parse().unwrap()),
        "Incorrect stored token"
    );
    assert_eq!(
        stored_transfer.amount,
        U128(DEFAULT_TRANSFER_AMOUNT),
        "Incorrect stored amount"
    );
}

#[test]
#[should_panic(expected = "ERR_INVALID_FEE")]
fn test_init_transfer_invalid_fee() {
    let mut contract = get_default_contract();
    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        &BridgeOnTransferMsg::InitTransfer(get_init_transfer_msg(
            DEFAULT_ETH_USER_ADDRESS,
            DEFAULT_TRANSFER_AMOUNT + 1,
            0,
        )),
    );
}

#[test]
fn test_init_transfer_balance_updated() {
    let mut contract = get_default_contract();

    let min_storage_balance = contract.required_balance_for_account();
    let init_transfer_balance = contract.required_balance_for_init_transfer(None);
    let total_balance = min_storage_balance.saturating_add(init_transfer_balance);

    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        Some(total_balance),
        &BridgeOnTransferMsg::InitTransfer(get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS, 0, 0)),
    );

    let storage_balance = contract
        .accounts_balances
        .get(&AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap())
        .unwrap();
    assert!(
        storage_balance.available < total_balance,
        "Expected storage balance must be deducted"
    );
}

#[test]
fn test_init_transfer_tracks_locked_tokens_per_chain() {
    let mut contract = get_default_contract();

    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        &BridgeOnTransferMsg::InitTransfer(get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS, 0, 0)),
    );

    let locked = contract.get_locked_tokens(
        ChainKind::Eth,
        AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
    );
    assert_eq!(locked, U128(DEFAULT_TRANSFER_AMOUNT));
}

fn run_update_transfer_fee(
    contract: &mut Contract,
    sender_id: String,
    init_fee: &Fee,
    new_fee: UpdateFee,
    attached_deposit: Option<NearToken>,
    new_sender_id: Option<String>,
) {
    use std::str::FromStr;

    let transfer_msg = TransferMessage {
        origin_nonce: DEFAULT_NONCE,
        token: OmniAddress::Near(
            AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        ),
        amount: U128(DEFAULT_TRANSFER_AMOUNT),
        recipient: OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
        fee: init_fee.clone(),
        sender: OmniAddress::Near(sender_id.clone().parse().unwrap()),
        msg: String::new(),
        destination_nonce: 1,
        origin_transfer_id: None,
    };

    contract.insert_raw_transfer(
        transfer_msg.clone(),
        AccountId::try_from(sender_id.clone()).unwrap(),
    );

    let attached_deposit = attached_deposit.unwrap_or_else(|| match &new_fee {
        UpdateFee::Fee(new_fee) => {
            NearToken::from_yoctonear(new_fee.native_fee.0.saturating_sub(init_fee.native_fee.0))
        }
        UpdateFee::Proof(_) => panic!("Not supported fee type"),
    });

    setup_test_env(
        AccountId::try_from(new_sender_id.unwrap_or(sender_id)).unwrap(),
        attached_deposit,
        None,
    );
    contract.update_transfer_fee(transfer_msg.get_transfer_id(), new_fee);
}

#[test]
fn test_update_transfer_fee_same_fee() {
    let mut contract = get_default_contract();

    let fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &fee,
        UpdateFee::Fee(fee.clone()),
        Some(NearToken::from_yoctonear(0)),
        None,
    );

    let updated_transfer = contract.get_transfer_message(DEFAULT_TRANSFER_ID);
    assert_eq!(updated_transfer.fee, fee);
}

#[test]
fn test_update_transfer_fee_valid() {
    let mut contract = get_default_contract();

    let fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(15),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &fee,
        UpdateFee::Fee(new_fee.clone()),
        None,
        None,
    );

    let updated_transfer = contract.get_transfer_message(DEFAULT_TRANSFER_ID);
    assert_eq!(updated_transfer.fee, new_fee);
}

#[test]
#[should_panic(expected = "ERR_INVALID_FEE")]
fn test_update_transfer_fee_exceeds_amount() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT + 1), // Fee larger than amount
        native_fee: U128(10),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &init_fee,
        UpdateFee::Fee(new_fee),
        None,
        None,
    );
}

#[test]
#[should_panic(expected = "ERR_LOWER_FEE")]
fn test_update_transfer_fee_lower_native_fee() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(5), // Lower native fee
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &init_fee,
        UpdateFee::Fee(new_fee),
        None,
        None,
    );
}

#[test]
#[should_panic(expected = "ERR_INVALID_ATTACHED_DEPOSIT")]
fn test_update_transfer_fee_invalid_deposit() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(15),
    };

    // Attached deposit doesn't match native fee difference
    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &init_fee,
        UpdateFee::Fee(new_fee),
        Some(NearToken::from_yoctonear(2)), // Wrong deposit amount
        None,
    );
}

#[test]
#[should_panic(expected = "Only sender can update token fee")]
fn test_update_transfer_fee_wrong_sender() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(10),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(), // Original sender
        &init_fee,
        UpdateFee::Fee(new_fee.clone()),
        None,
        Some("different_user.testnet".to_string()),
    );
}

#[test]
fn test_update_transfer_native_fee_different_sender() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(15),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        &init_fee,
        UpdateFee::Fee(new_fee.clone()),
        None,
        Some("different_user.testnet".to_string()),
    );
}

fn get_default_storage_deposit_actions() -> Vec<StorageDepositAction> {
    vec![StorageDepositAction {
        token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
        storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
    }]
}

fn get_prover_result(recipient: Option<OmniAddress>) -> ProverResult {
    use std::str::FromStr;
    let recipient = recipient.unwrap_or(OmniAddress::Near(
        DEFAULT_NEAR_USER_ACCOUNT.parse().unwrap(),
    ));
    ProverResult::InitTransfer(InitTransferMessage {
        origin_nonce: DEFAULT_NONCE,
        token: OmniAddress::Near(
            AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        ),
        amount: U128(DEFAULT_TRANSFER_AMOUNT),
        recipient,
        fee: Fee {
            fee: U128(10),
            native_fee: U128(5),
        },
        sender: OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
        msg: String::new(),
        emitter_address: OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    })
}

#[test]
fn test_fin_transfer_callback_near_success() {
    let mut contract = get_default_contract();
    contract.factories.insert(
        &ChainKind::Eth,
        &OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    );

    let native_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
    contract.token_address_to_id.insert(
        &native_token_address,
        &DEFAULT_FT_CONTRACT_ACCOUNT.parse().unwrap(),
    );
    contract.locked_tokens.insert(
        &(
            ChainKind::Eth,
            AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        ),
        &DEFAULT_TRANSFER_AMOUNT,
    );
    contract.token_decimals.insert(
        &OmniAddress::Near(AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap()),
        &Decimals {
            decimals: 24,
            origin_decimals: 24,
        },
    );

    let storage_actions = vec![
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
    ];

    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    let prover_result = get_prover_result(Some(OmniAddress::Near(
        DEFAULT_NEAR_USER_ACCOUNT.parse().unwrap(),
    )));

    setup_test_env(
        predecessor.clone(),
        NearToken::from_near(1),
        Some(vec![
            PromiseResult::Successful(borsh::to_vec(&prover_result).unwrap()),
            // Storage balance result for transfer recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
            // Storage balance result for fee recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
            // Storage balance result for native fee recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
        ]),
    );

    let result = contract.fin_transfer_callback(&storage_actions, predecessor.clone());

    assert!(matches!(result, PromiseOrValue::Promise(_)));
    assert_eq!(
        contract.get_locked_tokens(
            ChainKind::Eth,
            AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        ),
        U128(0)
    );
}

#[test]
#[should_panic(expected = "ERR_INSUFFICIENT_LOCKED_TOKENS")]
fn test_fin_transfer_callback_near_fails_without_locked_tokens() {
    let mut contract = get_default_contract();
    contract.factories.insert(
        &ChainKind::Eth,
        &OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    );

    let native_token_address = OmniAddress::new_zero(ChainKind::Eth).unwrap();
    contract.token_address_to_id.insert(
        &native_token_address,
        &DEFAULT_FT_CONTRACT_ACCOUNT.parse().unwrap(),
    );
    contract.token_decimals.insert(
        &OmniAddress::Near(AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap()),
        &Decimals {
            decimals: 24,
            origin_decimals: 24,
        },
    );

    // Only 1 token locked while 100 are requested.
    contract.locked_tokens.insert(
        &(
            ChainKind::Eth,
            AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        ),
        &1,
    );

    let storage_actions = vec![
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
        StorageDepositAction {
            token_id: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
            account_id: AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
            storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
        },
    ];

    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    let prover_result = get_prover_result(Some(OmniAddress::Near(
        DEFAULT_NEAR_USER_ACCOUNT.parse().unwrap(),
    )));

    setup_test_env(
        predecessor.clone(),
        NearToken::from_near(1),
        Some(vec![
            PromiseResult::Successful(borsh::to_vec(&prover_result).unwrap()),
            // Storage balance result for transfer recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
            // Storage balance result for fee recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
            // Storage balance result for native fee recipient
            PromiseResult::Successful(
                serde_json::to_vec(&Some(StorageBalance {
                    total: NearToken::from_near(1),
                    available: NearToken::from_near(1),
                }))
                .unwrap(),
            ),
        ]),
    );

    // Should panic because locked balance is insufficient.
    let _ = contract.fin_transfer_callback(&storage_actions, predecessor.clone());
}

#[test]
fn test_fin_transfer_callback_non_near_success() {
    use std::str::FromStr;

    let mut contract = get_default_contract();
    contract.factories.insert(
        &ChainKind::Eth,
        &OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    );
    let storage_actions = get_default_storage_deposit_actions();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    // Create prover result with ETH recipient
    let eth_recipient = OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap());
    let prover_result = get_prover_result(Some(eth_recipient.clone()));

    contract.token_decimals.insert(
        &OmniAddress::Near(AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap()),
        &Decimals {
            decimals: 24,
            origin_decimals: 24,
        },
    );

    setup_test_env(
        predecessor.clone(),
        NearToken::from_near(1),
        Some(vec![PromiseResult::Successful(
            borsh::to_vec(&prover_result).unwrap(),
        )]),
    );

    let result = contract.fin_transfer_callback(&storage_actions, predecessor.clone());

    // For non-NEAR recipients, should return u64 value of current_destination_nonce
    match result {
        PromiseOrValue::Value(nonce) => {
            assert_eq!(
                nonce,
                contract.get_current_destination_nonce(ChainKind::Eth)
            );

            // Verify transfer was stored correctly
            let stored_transfer = contract.get_transfer_message(TransferId {
                origin_chain: ChainKind::Eth,
                origin_nonce: DEFAULT_NONCE,
            });
            assert_eq!(stored_transfer.recipient, eth_recipient);
        }
        PromiseOrValue::Promise(_) => panic!("Expected Value variant, got Promise"),
    }
}

#[test]
#[should_panic(expected = "Invalid proof message")]
fn test_fin_transfer_callback_invalid_proof() {
    let mut contract = get_default_contract();
    let storage_actions = get_default_storage_deposit_actions();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    testing_env!(
        VMContextBuilder::new()
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(NearToken::from_near(1))
            .build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Failed],
    );

    contract
        .fin_transfer_callback(&storage_actions, predecessor)
        .detach();
}

#[test]
#[should_panic(expected = "Unknown factory")]
fn test_fin_transfer_callback_unknown_factory() {
    let mut contract = get_default_contract();
    let storage_actions = get_default_storage_deposit_actions();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    // Don't add factory to make it fail

    testing_env!(
        VMContextBuilder::new()
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(NearToken::from_near(1))
            .build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(
            borsh::to_vec(&get_prover_result(None)).unwrap()
        )],
    );

    contract
        .fin_transfer_callback(&storage_actions, predecessor)
        .detach();
}

#[test]
fn test_fin_transfer_callback_refund_restores_locked_tokens() {
    use std::str::FromStr;

    let mut contract = get_default_contract();
    let token_id = AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap();
    let recipient =
        AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).expect("Invalid account");
    let fee_recipient = recipient.clone();

    let transfer_message = TransferMessage {
        origin_nonce: DEFAULT_NONCE,
        token: OmniAddress::Near(token_id.clone()),
        amount: U128(DEFAULT_TRANSFER_AMOUNT),
        recipient: OmniAddress::Near(recipient.clone()),
        fee: Fee {
            fee: U128(0),
            native_fee: U128(0),
        },
        sender: OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
        msg: "refund".to_string(),
        destination_nonce: 1,
        origin_transfer_id: None,
    };

    setup_test_env(
        recipient.clone(),
        NearToken::from_near(0),
        Some(vec![PromiseResult::Successful(
            serde_json::to_vec(&U128(0)).unwrap(),
        )]),
    );

    contract.fin_transfer_send_tokens_callback(transfer_message, &fee_recipient, true, &recipient);

    assert_eq!(
        contract.get_locked_tokens(ChainKind::Eth, token_id),
        U128(DEFAULT_TRANSFER_AMOUNT)
    );
}

#[test]
fn test_is_transfer_finalised() {
    let mut contract = get_default_contract();
    let chain = ChainKind::Eth;
    let nonce = 1;
    let transfer_id = TransferId {
        origin_chain: chain,
        origin_nonce: nonce,
    };

    assert!(!contract.is_transfer_finalised(transfer_id));

    contract.finalised_transfers.insert(&transfer_id);
    assert!(contract.is_transfer_finalised(transfer_id));
}

#[test]
fn test_normalize_amount() {
    assert_eq!(
        Contract::normalize_amount(
            u128::MAX,
            Decimals {
                decimals: 18,
                origin_decimals: 18
            }
        ),
        u128::MAX
    );

    assert_eq!(
        Contract::normalize_amount(
            u128::MAX,
            Decimals {
                decimals: 18,
                origin_decimals: 24
            }
        ),
        u128::MAX / 1_000_000
    );

    assert_eq!(
        Contract::normalize_amount(
            u128::MAX,
            Decimals {
                decimals: 9,
                origin_decimals: 24
            }
        ),
        u128::MAX / 1_000_000_000_000_000
    );
}

#[test]
fn test_denormalize_amount() {
    assert_eq!(
        Contract::denormalize_amount(
            u128::MAX,
            Decimals {
                decimals: 18,
                origin_decimals: 18
            }
        ),
        u128::MAX
    );

    assert_eq!(
        Contract::denormalize_amount(
            u64::MAX.into(),
            Decimals {
                decimals: 18,
                origin_decimals: 24
            }
        ),
        u128::from(u64::MAX) * 1_000_000_u128
    );

    assert_eq!(
        Contract::denormalize_amount(
            u64::MAX.into(),
            Decimals {
                decimals: 9,
                origin_decimals: 24
            }
        ),
        u128::from(u64::MAX) * 1_000_000_000_000_000_u128
    );
}

#[test]
fn test_get_bridged_token() {
    let mut contract = get_default_contract();

    // Set up test data
    let near_token_id: AccountId = DEFAULT_FT_CONTRACT_ACCOUNT.parse().unwrap();
    let eth_address = EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap();
    let solana_address: SolAddress = "2xNweLHLqbS9YpP3UyaPrxKqgqoC6yPBFyuLxA8qtgr4"
        .parse()
        .expect("Invalid Solana address");

    // First insert token addresses for each chain target (NEAR token -> chain addresses)
    contract.token_id_to_address.insert(
        &(ChainKind::Eth, near_token_id.clone()),
        &OmniAddress::Eth(eth_address.clone()),
    );
    contract.token_id_to_address.insert(
        &(ChainKind::Sol, near_token_id.clone()),
        &OmniAddress::Sol(solana_address.clone()),
    );

    // Then insert reverse mappings (chain addresses -> NEAR token)
    contract.token_address_to_id.insert(
        &OmniAddress::Eth(eth_address.clone()),
        &near_token_id.clone(),
    );
    contract.token_address_to_id.insert(
        &OmniAddress::Sol(solana_address.clone()),
        &near_token_id.clone(),
    );

    // Test Case 1: NEAR to Ethereum
    let near_source = OmniAddress::Near(near_token_id.clone());
    let eth_result = contract.get_bridged_token(&near_source, ChainKind::Eth);
    assert_eq!(
        eth_result,
        Some(OmniAddress::Eth(eth_address.clone())),
        "Failed to resolve NEAR to ETH address"
    );

    // Test Case 2: NEAR to Solana
    let solana_result = contract.get_bridged_token(&near_source, ChainKind::Sol);
    assert_eq!(
        solana_result,
        Some(OmniAddress::Sol(solana_address.clone())),
        "Failed to resolve NEAR to Solana address"
    );

    // Test Case 3: Ethereum to NEAR
    let eth_source = OmniAddress::Eth(eth_address.clone());
    let near_result = contract.get_bridged_token(&eth_source, ChainKind::Near);
    assert_eq!(
        near_result,
        Some(OmniAddress::Near(near_token_id.clone())),
        "Failed to resolve ETH to NEAR address"
    );

    // Test Case 4: Ethereum to Solana (cross-chain)
    let solana_cross_result = contract.get_bridged_token(&eth_source, ChainKind::Sol);
    assert_eq!(
        solana_cross_result,
        Some(OmniAddress::Sol(solana_address.clone())),
        "Failed to resolve ETH to Solana address"
    );

    // Test Case 5: Unmapped token
    let unmapped_eth = OmniAddress::Eth(
        EvmAddress::from_str("0x9999999999999999999999999999999999999999").unwrap(),
    );
    let unmapped_result = contract.get_bridged_token(&unmapped_eth, ChainKind::Sol);
    assert_eq!(
        unmapped_result, None,
        "Expected None for unmapped token address"
    );

    // Test Case 6: Same chain resolution attempt
    let same_chain_result = contract.get_bridged_token(&eth_source, ChainKind::Eth);
    assert_eq!(
        same_chain_result,
        Some(OmniAddress::Eth(eth_address.clone())),
        "Failed to handle same chain resolution"
    );

    // Test Case 7:  NEAR -> NEAR (no storage needed)
    assert_eq!(
        contract.get_bridged_token(&near_source, ChainKind::Near),
        Some(near_source.clone()),
        "Failed to handle NEAR to NEAR resolution"
    );
}

#[test]
fn test_legacy_ft_on_transfer() {
    let mut contract = get_default_contract();

    run_ft_on_transfer_legacy(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(100),
        None,
        &get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS, 0, 0),
    );
}
