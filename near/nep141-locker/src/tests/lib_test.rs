use near_contract_standards::storage_management::StorageBalance;

use crate::Contract;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::RuntimeFeesConfig;
use near_sdk::{test_vm_config, testing_env};
use omni_types::prover_result::{InitTransferMessage, ProverResult};
use omni_types::{EvmAddress, NativeFee, Nonce, TransferId};
use std::str::FromStr;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh;
use near_sdk::json_types::U128;
use near_sdk::{serde_json, AccountId, NearToken, PromiseOrValue, PromiseResult};
use omni_types::locker_args::StorageDepositArgs;
use omni_types::{ChainKind, Fee, InitTransferMsg, OmniAddress, TransferMessage, UpdateFee};

const DEFAULT_NONCE: Nonce = 0;
const DEFAULT_TRANSFER_ID: TransferId = TransferId {
    origin_chain: ChainKind::Near,
    origin_nonce: DEFAULT_NONCE,
};
const DEFAULT_PROVER_ACCOUNT: &str = "prover.testnet";
const DEFAULT_MPC_SIGNER_ACCOUNT: &str = "mpc_signer.testnet";
const DEFAULT_WNEAR_ACCOUNT: &str = "wnear.testnet";

const DEFAULT_NEAR_USER_ACCOUNT: &str = "user.testnet";
const DEFAULT_FT_CONTRACT_ACCOUNT: &str = "ft_contract.testnet";
const DEFAULT_ETH_USER_ADDRESS: &str = "0x1234567890123456789012345678901234567890";
const DEFAULT_TRANSFER_AMOUNT: u128 = 100;

fn setup_test_env(
    predecessor_account_id: AccountId,
    attached_deposit: NearToken,
    promise_results: Option<Vec<PromiseResult>>,
) {
    let context = VMContextBuilder::new()
        .predecessor_account_id(predecessor_account_id)
        .attached_deposit(attached_deposit)
        .build();

    if let Some(results) = promise_results {
        testing_env!(
            context,
            test_vm_config(),
            RuntimeFeesConfig::test(),
            Default::default(),
            results,
        );
    } else {
        testing_env!(context);
    }
}

fn setup_contract(prover_id: String, mpc_signer_id: String, wnear_id: String) -> Contract {
    Contract::new(
        AccountId::try_from(prover_id).expect("Invalid default prover ID"),
        AccountId::try_from(mpc_signer_id).expect("Invalid default mpc signer ID"),
        AccountId::try_from(wnear_id).expect("Invalid default wnear ID"),
    )
}

fn get_default_contract() -> Contract {
    setup_contract(
        DEFAULT_PROVER_ACCOUNT.to_string(),
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

fn get_init_transfer_msg(recipient: String, fee: u128, native_token_fee: u128) -> InitTransferMsg {
    InitTransferMsg {
        recipient: OmniAddress::Eth(EvmAddress::from_str(&recipient).unwrap()),
        fee: U128(fee),
        native_token_fee: U128(native_token_fee),
    }
}

fn run_ft_on_transfer(
    contract: &mut Contract,
    sender_id: String,
    token_id: String,
    amount: U128,
    attached_deposit: Option<NearToken>,
    msg: InitTransferMsg,
) -> PromiseOrValue<U128> {
    let sender_id = AccountId::try_from(sender_id).expect("Invalid sender ID");
    let token_id = AccountId::try_from(token_id).expect("Invalid token ID");

    let attached_deposit = match attached_deposit {
        Some(deposit) => deposit,
        None => {
            let min_storage_balance = contract.required_balance_for_account();
            let init_transfer_balance = contract.required_balance_for_init_transfer();
            min_storage_balance.saturating_add(init_transfer_balance)
        }
    };

    run_storage_deposit(contract, sender_id.clone(), attached_deposit);
    setup_test_env(token_id.clone(), NearToken::from_yoctonear(0), None);

    let msg = serde_json::to_string(&msg).expect("Failed to serialize transfer message");

    contract.ft_on_transfer(sender_id, amount, msg)
}

#[test]
fn test_initialize_contract() {
    let contract = get_default_contract();

    assert_eq!(contract.prover_account, DEFAULT_PROVER_ACCOUNT);
    assert_eq!(contract.mpc_signer, DEFAULT_MPC_SIGNER_ACCOUNT);
    assert_eq!(contract.current_origin_nonce, DEFAULT_NONCE);
    assert_eq!(contract.wnear_account_id, DEFAULT_WNEAR_ACCOUNT);
}

#[test]
fn test_ft_on_transfer_nonce_increment() {
    let mut contract = get_default_contract();

    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(100),
        None,
        get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS.to_string(), 0, 0),
    );

    assert_eq!(contract.current_origin_nonce, DEFAULT_NONCE + 1);
}

#[test]
fn test_ft_on_transfer_stored_transfer_message() {
    let mut contract = get_default_contract();

    let msg = get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS.to_string(), 0, 0);
    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        msg.clone(),
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
fn test_ft_on_transfer_promise_result() {
    let mut contract = get_default_contract();

    let promise = run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS.to_string(), 0, 0),
    );

    let remaining = match promise {
        PromiseOrValue::Value(remaining) => remaining,
        PromiseOrValue::Promise(_) => panic!("Expected Value variant, got Promise"),
    };
    assert_eq!(remaining, U128(0), "Expected remaining amount to be 0");
}

#[test]
#[should_panic(expected = "ERR_INVALID_FEE")]
fn test_ft_on_transfer_invalid_fee() {
    let mut contract = get_default_contract();
    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        None,
        get_init_transfer_msg(
            DEFAULT_ETH_USER_ADDRESS.to_string(),
            DEFAULT_TRANSFER_AMOUNT + 1,
            0,
        ),
    );
}

#[test]
fn test_ft_on_transfer_balance_updated() {
    let mut contract = get_default_contract();

    let min_storage_balance = contract.required_balance_for_account();
    let init_transfer_balance = contract.required_balance_for_init_transfer();
    let total_balance = min_storage_balance.saturating_add(init_transfer_balance);

    run_ft_on_transfer(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(),
        DEFAULT_FT_CONTRACT_ACCOUNT.to_string(),
        U128(DEFAULT_TRANSFER_AMOUNT),
        Some(total_balance),
        get_init_transfer_msg(DEFAULT_ETH_USER_ADDRESS.to_string(), 0, 0),
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

fn run_update_transfer_fee(
    contract: &mut Contract,
    sender_id: String,
    init_fee: Fee,
    new_fee: UpdateFee,
    attached_deposit: Option<NearToken>,
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
        msg: "".to_string(),
        destination_nonce: 1,
    };

    contract.insert_raw_transfer(
        transfer_msg.clone(),
        AccountId::try_from(sender_id.clone()).unwrap(),
    );

    let attached_deposit = attached_deposit.unwrap_or_else(|| match &new_fee {
        UpdateFee::Fee(new_fee) => {
            NearToken::from_yoctonear(new_fee.native_fee.0.saturating_sub(init_fee.native_fee.0))
        }
        _ => panic!("Not supported fee type"),
    });

    setup_test_env(
        AccountId::try_from(sender_id).unwrap(),
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
        fee.clone(),
        UpdateFee::Fee(fee.clone()),
        Some(NearToken::from_yoctonear(0)),
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
        fee.clone(),
        UpdateFee::Fee(new_fee.clone()),
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
        init_fee,
        UpdateFee::Fee(new_fee),
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
        init_fee,
        UpdateFee::Fee(new_fee),
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
        init_fee,
        UpdateFee::Fee(new_fee),
        Some(NearToken::from_yoctonear(2)), // Wrong deposit amount
    );
}

#[test]
#[should_panic(expected = "Only sender can update fee")]
fn test_update_transfer_fee_wrong_sender() {
    let mut contract = get_default_contract();

    let init_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 2),
        native_fee: U128(10),
    };

    let new_fee = Fee {
        fee: U128(DEFAULT_TRANSFER_AMOUNT - 1),
        native_fee: U128(15),
    };

    run_update_transfer_fee(
        &mut contract,
        DEFAULT_NEAR_USER_ACCOUNT.to_string(), // Original sender
        init_fee,
        UpdateFee::Fee(new_fee.clone()),
        None,
    );

    // Try to update with different sender
    setup_test_env(
        AccountId::try_from("different_user.testnet".to_string()).unwrap(),
        NearToken::from_yoctonear(5),
        None,
    );
    contract.update_transfer_fee(DEFAULT_TRANSFER_ID, UpdateFee::Fee(new_fee));
}

fn get_default_storage_deposit_args() -> StorageDepositArgs {
    StorageDepositArgs {
        token: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        accounts: vec![],
    }
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
        msg: "".to_string(),
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

    // Add both recipient and fee recipient to storage deposit args
    let storage_args = StorageDepositArgs {
        token: AccountId::try_from(DEFAULT_FT_CONTRACT_ACCOUNT.to_string()).unwrap(),
        accounts: vec![
            // Transfer recipient
            (
                AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
                true,
            ),
            // Fee recipient
            (
                AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap(),
                true,
            ),
        ],
    };

    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();
    let native_fee_recipient = Some(OmniAddress::Eth(
        EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap(),
    ));

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
        ]),
    );

    let result =
        contract.fin_transfer_callback(&storage_args, predecessor.clone(), native_fee_recipient);

    assert!(matches!(result, PromiseOrValue::Promise(_)));
}

#[test]
fn test_fin_transfer_callback_non_near_success() {
    use std::str::FromStr;

    let mut contract = get_default_contract();
    contract.factories.insert(
        &ChainKind::Eth,
        &OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    );
    let storage_args = get_default_storage_deposit_args();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    // Create prover result with ETH recipient
    let eth_recipient = OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap());
    let prover_result = get_prover_result(Some(eth_recipient.clone()));

    setup_test_env(
        predecessor.clone(),
        NearToken::from_near(1),
        Some(vec![PromiseResult::Successful(
            borsh::to_vec(&prover_result).unwrap(),
        )]),
    );

    let result = contract.fin_transfer_callback(&storage_args, predecessor.clone(), None);

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
    let storage_args = get_default_storage_deposit_args();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    testing_env!(
        VMContextBuilder::new()
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(NearToken::from_near(1))
            .build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Failed],
    );

    contract.fin_transfer_callback(&storage_args, predecessor, None);
}

#[test]
#[should_panic(expected = "Unknown factory")]
fn test_fin_transfer_callback_unknown_factory() {
    let mut contract = get_default_contract();
    let storage_args = get_default_storage_deposit_args();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    // Don't add factory to make it fail

    testing_env!(
        VMContextBuilder::new()
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(NearToken::from_near(1))
            .build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Successful(
            borsh::to_vec(&get_prover_result(None)).unwrap()
        )],
    );

    contract.fin_transfer_callback(&storage_args, predecessor, None);
}

#[test]
#[should_panic(expected = "ERR_FEE_RECIPIENT_NOT_SET")]
fn test_fin_transfer_callback_missing_fee_recipient() {
    let mut contract = get_default_contract();

    // Add factory
    contract.factories.insert(
        &ChainKind::Eth,
        &OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    );

    let mut prover_result = get_prover_result(None);
    if let ProverResult::InitTransfer(ref mut init_transfer) = prover_result {
        init_transfer.fee.native_fee = U128(100); // Set non-zero native fee
    }

    let storage_args = get_default_storage_deposit_args();
    let predecessor = AccountId::try_from(DEFAULT_NEAR_USER_ACCOUNT.to_string()).unwrap();

    testing_env!(
        VMContextBuilder::new()
            .predecessor_account_id(predecessor.clone())
            .attached_deposit(NearToken::from_near(1))
            .build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![PromiseResult::Successful(
            borsh::to_vec(&prover_result).unwrap()
        )],
    );

    contract.fin_transfer_callback(
        &storage_args,
        predecessor,
        None, // Missing fee recipient when native fee is non-zero
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

    contract.finalised_transfers.insert(&transfer_id, &None);
    assert!(contract.is_transfer_finalised(transfer_id));

    let native_fee = NativeFee {
        amount: U128(100),
        recipient: OmniAddress::Eth(EvmAddress::from_str(DEFAULT_ETH_USER_ADDRESS).unwrap()),
    };
    contract
        .finalised_transfers
        .insert(&transfer_id, &Some(native_fee));
    assert!(contract.is_transfer_finalised(transfer_id));
}
