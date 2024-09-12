use btc_types::contract_args::{InitArgs, ProofArgs};
use btc_types::header::{ExtendedHeader, Header};
use near_sdk::NearToken;
use serde_json::json;

const STORAGE_DEPOSIT_PER_BLOCK: NearToken = NearToken::from_millinear(500);

#[tokio::test]
async fn test_setting_genesis_block() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract_wasm = near_workspaces::compile_project("./").await?;

    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let block_header = genesis_block_header();
    let args = InitArgs {
        genesis_block: block_header.clone(),
        genesis_block_hash: block_header.block_hash(),
        genesis_block_height: 0,
        skip_pow_verification: true,
        gc_threshold: 5,
    };
    // Call the init method on the contract
    let outcome = contract
        .call("init")
        .args_json(json!({
            "args": serde_json::to_value(args).unwrap(),
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    let _user_account = sandbox.dev_create_account().await?;

    let user_message_outcome = contract
        .view("get_last_block_header")
        .args_json(json!({}))
        .await?;

    assert_eq!(
        user_message_outcome.json::<ExtendedHeader>()?.block_header,
        block_header.clone()
    );

    Ok(())
}

#[tokio::test]
async fn test_setting_chain_reorg() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract_wasm = near_workspaces::compile_project("./").await?;

    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let block_header = genesis_block_header();

    let args = InitArgs {
        genesis_block_hash: block_header.block_hash(),
        genesis_block: block_header.clone(),
        genesis_block_height: 0,
        skip_pow_verification: true,
        gc_threshold: 5,
    };
    // Call the init method on the contract
    let outcome = contract
        .call("init")
        .args_json(json!({
            "args": serde_json::to_value(args).unwrap(),
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    let user_account = sandbox.dev_create_account().await?;

    // second block
    let outcome = user_account
        .call(contract.id(), "submit_blocks")
        .args_borsh([block_header_example()].to_vec())
        .deposit(STORAGE_DEPOSIT_PER_BLOCK)
        .transact()
        .await?;
    assert!(outcome.is_success());

    // first fork block
    let outcome = user_account
        .call(contract.id(), "submit_blocks")
        .args_borsh([fork_block_header_example()].to_vec())
        .deposit(STORAGE_DEPOSIT_PER_BLOCK)
        .transact()
        .await?;
    assert!(outcome.is_success());

    // second fork block
    let outcome = user_account
        .call(contract.id(), "submit_blocks")
        .args_borsh([fork_block_header_example_2()].to_vec())
        .deposit(STORAGE_DEPOSIT_PER_BLOCK)
        .transact()
        .await?;
    assert!(outcome.is_success());

    let user_message_outcome = contract
        .view("get_last_block_header")
        .args_json(json!({}))
        .await?;

    assert_eq!(
        user_message_outcome.json::<ExtendedHeader>()?.block_header,
        fork_block_header_example_2()
    );

    Ok(())
}

#[tokio::test]
async fn test_view_call_verify_transaction_inclusion() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract_wasm = near_workspaces::compile_project("./").await?;

    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let block_header = genesis_block_header();
    let args = InitArgs {
        genesis_block: block_header.clone(),
        genesis_block_hash: block_header.block_hash(),
        genesis_block_height: 0,
        skip_pow_verification: true,
        gc_threshold: 5,
    };
    // Call the init method on the contract
    let outcome = contract
        .call("init")
        .args_json(json!({
            "args": serde_json::to_value(args).unwrap(),
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    let user_account = sandbox.dev_create_account().await?;
    let result: bool = user_account
        .view(contract.id(), "verify_transaction_inclusion")
        .args_borsh(ProofArgs {
            tx_id: merkle_tools::H256::default(),
            tx_block_blockhash: block_header.block_hash(),
            tx_index: 0,
            merkle_proof: vec![],
            confirmations: 0,
        })
        .await?
        .json()?;

    assert!(!result);

    Ok(())
}

fn genesis_block_header() -> Header {
    let json_value = serde_json::json!({
        "version": 1,
        "prev_block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
        "time": 1_231_006_505,
        "bits": 486_604_799,
        "nonce": 2_083_236_893
    });

    serde_json::from_value(json_value).expect("value is invalid")
}

// Bitcoin header example
fn block_header_example() -> Header {
    let json_value = serde_json::json!({
        // block_hash: 62703463e75c025987093c6fa96e7261ac982063ea048a0550407ddbbe865345
        "version": 1,
        "prev_block_hash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
        "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
        "time": 1_231_006_506,
        "bits": 486_604_799,
        "nonce": 2_083_236_893
    });

    serde_json::from_value(json_value).expect("value is invalid")
}

fn fork_block_header_example() -> Header {
    let json_value = serde_json::json!({
        // "hash": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048",
        //"chainwork": "0000000000000000000000000000000000000000000000000000000200020002",
        "version": 1,
        "merkle_root": "0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098",
        "time": 1_231_469_665,
        "nonce": 2_573_394_689_u32,
        "bits": 486_604_799,
        "prev_block_hash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
    });

    serde_json::from_value(json_value).expect("value is invalid")
}

fn fork_block_header_example_2() -> Header {
    let json_value = serde_json::json!({
        // "hash": "000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd",
        // "chainwork": "0000000000000000000000000000000000000000000000000000000300030003",
      "version": 1,
      "merkle_root": "9b0fc92260312ce44e74ef369f5c66bbb85848f2eddd5a7a1cde251e54ccfdd5",
      "time": 1_231_469_744,
      "nonce": 1_639_830_024,
      "bits": 486_604_799,
      "prev_block_hash": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048",
    });

    serde_json::from_value(json_value).expect("value is invalid")
}
