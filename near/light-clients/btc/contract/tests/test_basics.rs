use bitcoin::block::Header;
use serde_json::json;

#[tokio::test]
async fn test_setting_block_header() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract_wasm = near_workspaces::compile_project("./").await?;

    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let user_account = sandbox.dev_create_account().await?;

    let block_header = block_header_example();

    let outcome = user_account
        .call(contract.id(), "submit_block_header")
        .args_json(json!({
            "block_header": {
            "version": 1,
            "prev_blockhash":"0000000000000000000000000000000000000000000000000000000000000000",
            "merkle_root":"4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "time":1231006505,
            "bits":486604799,
            "nonce":2083236893},
            "height": 2
        }))
        .transact()
        .await?;
    eprint!("{:?}", outcome);
    assert!(outcome.is_success());

    let user_message_outcome = contract
        .view("get_last_block_header")
        .args_json(json!({}))
        .await?;
    assert_eq!(user_message_outcome.json::<Header>()?, block_header);

    Ok(())
}

fn block_header_example() -> Header {
    let json_value = json!({
            "block_header": 123,
            "version": 1,
            "prev_blockhash":"0000000000000000000000000000000000000000000000000000000000000000",
            "merkle_root":"4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "time":1231006505,
            "bits":486604799,
            "nonce":2083236893
        });
    let parsed_header = serde_json::from_value(json_value).expect("value is invalid");
    parsed_header
}