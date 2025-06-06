#[cfg(feature = "bitcoin")]
mod test_basics {
    use btc_types::contract_args::{InitArgs, ProofArgs};
    use btc_types::hash::H256;
    use btc_types::header::{ExtendedHeader, Header};
    use near_sdk::NearToken;
    use near_workspaces::{Account, Contract};
    use serde_json::json;
    use std::fs::File;
    use std::io::BufReader;

    const STORAGE_DEPOSIT_PER_BLOCK: NearToken = NearToken::from_millinear(500);

    async fn init_contract() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = near_workspaces::compile_project("./").await?;

        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let block_header = genesis_block_header();
        let args = InitArgs {
            genesis_block_hash: block_header.block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: true,
            gc_threshold: 5,
            network: btc_types::network::Network::Mainnet,
            submit_blocks: [block_header.clone()].to_vec(),
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

        Ok((contract, user_account))
    }

    async fn init_contract_from_file(
        gc_threshold: u64,
    ) -> Result<(Contract, Account, Vec<Vec<Header>>), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = near_workspaces::compile_project("./").await?;

        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let block_headers =
            read_blocks_from_json("./tests/data/blocks_headers_685440-687456_mainnet.json");
        let args = InitArgs {
            genesis_block_hash: block_headers[0][0].block_hash(),
            genesis_block_height: 685_440,
            skip_pow_verification: false,
            gc_threshold,
            network: btc_types::network::Network::Mainnet,
            submit_blocks: [block_headers[0][0].clone()].to_vec(),
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

        Ok((contract, user_account, block_headers))
    }

    #[tokio::test]
    async fn test_setting_genesis_block() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, _user_account) = init_contract().await?;

        let outcome = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?;

        assert_eq!(
            outcome.json::<ExtendedHeader>()?.block_header,
            genesis_block_header().clone()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_setting_chain_reorg() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;

        let storage_usage_init = contract.view_account().await.unwrap().storage_usage;
        // second block
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([block_header_example()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_one_block = contract.view_account().await.unwrap().storage_usage;

        // first fork block
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([fork_block_header_example()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_fork = contract.view_account().await.unwrap().storage_usage;

        // second fork block
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([fork_block_header_example_2()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_after = contract.view_account().await.unwrap().storage_usage;
        assert_eq!(
            storage_usage_after - storage_usage_fork,
            storage_usage_one_block
                - storage_usage_init
                - (storage_usage_fork - storage_usage_one_block)
        );

        let outcome = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?;

        assert_eq!(
            outcome.json::<ExtendedHeader>()?.block_header,
            fork_block_header_example_2()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_view_call_verify_transaction_inclusion() -> Result<(), Box<dyn std::error::Error>>
    {
        let (contract, user_account) = init_contract().await?;

        let result: bool = user_account
            .view(contract.id(), "verify_transaction_inclusion")
            .args_borsh(ProofArgs {
                tx_id: merkle_tools::H256::default(),
                tx_block_blockhash: genesis_block_header().block_hash(),
                tx_index: 0,
                merkle_proof: vec![],
                confirmations: 0,
            })
            .await?
            .json()?;

        assert!(!result);

        Ok(())
    }

    fn read_blocks_from_json(path: &str) -> Vec<Vec<Header>> {
        let file = File::open(path).expect("Unable to open file");
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap()
    }

    #[tokio::test]
    async fn test_submit_blocks_for_period() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account, block_headers) = init_contract_from_file(2017).await?;

        for block_headers_batch in &block_headers[1..] {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers_batch.clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            assert!(outcome.is_success());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_get_last_n_blocks() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account, block_headers) = init_contract_from_file(2017).await?;

        for block_headers_batch in &block_headers[1..=2] {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers_batch.clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            assert!(outcome.is_success());
        }

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 0, "limit": 0}))
            .await?;

        assert_eq!(outcome.json::<Vec<H256>>()?, vec![]);

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 0, "limit": 200}))
            .await?;

        assert_eq!(outcome.json::<Vec<H256>>()?.len(), 97);

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 200, "limit": 200}))
            .await?;

        assert_eq!(outcome.json::<Vec<H256>>()?, vec![]);

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 10, "limit": 10}))
            .await?;

        let last_blocks = outcome.json::<Vec<H256>>()?;
        assert_eq!(
            last_blocks[0].to_string(),
            "0000000000000000000aab4a401ac27b945057be99db4ccc9631da4bb0b9d746"
        );

        assert_eq!(
            last_blocks[9].to_string(),
            "0000000000000000000758a734884015e791dee8aced3dcce049753dc5aeeacb"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_gc() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account, block_headers) = init_contract_from_file(10).await?;

        let mut submitted_blocks_count: usize = 1;

        for block_headers_batch in &block_headers[1..=2] {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers_batch.clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            assert!(outcome.is_success());
            submitted_blocks_count += block_headers_batch.len();
        }
        assert_eq!(submitted_blocks_count, 97);

        let outcome = contract
            .view("get_mainchain_size")
            .args_json(json!({}))
            .await?;

        assert_eq!(outcome.json::<u64>().unwrap(), 10);

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 0, "limit": 100}))
            .await?;

        let mainchain_blocks = outcome.json::<Vec<H256>>().unwrap();
        assert_eq!(mainchain_blocks.len(), 10);
        for i in 0..mainchain_blocks.len() {
            assert_eq!(
                mainchain_blocks[mainchain_blocks.len() - i - 1],
                block_headers[2][block_headers[2].len() - i - 1].block_hash()
            );
        }

        let outcome = user_account
            .call(contract.id(), "run_mainchain_gc")
            .args_json(json!({"batch_size": 100}))
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success());

        let outcome = contract
            .view("get_mainchain_size")
            .args_json(json!({}))
            .await?;

        assert_eq!(outcome.json::<u64>().unwrap(), 10);
        Ok(())
    }

    #[tokio::test]
    async fn test_payment_on_block_submission() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account, block_headers) = init_contract_from_file(10).await?;

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[1].clone())
            .max_gas()
            .transact()
            .await?;

        assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
            .contains("Required deposit"));

        for block_headers_batch in block_headers.iter().take(3).skip(1) {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers_batch.clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            assert!(outcome.is_success());
        }

        let amount_init = user_account.view_account().await?.balance;

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[3].clone())
            .max_gas()
            .transact()
            .await?;

        assert!(outcome.is_success());

        let amount_before = user_account.view_account().await?.balance;
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[4].clone())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;

        assert!(outcome.is_success());

        let amount_after = user_account.view_account().await?.balance;
        assert!(
            amount_before.as_yoctonear() - amount_after.as_yoctonear()
                < 2 * (amount_init.as_yoctonear() - amount_before.as_yoctonear())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_submit_blocks_for_period_incorrect_target(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account, mut block_headers) = init_contract_from_file(2017).await?;

        for i in 0..block_headers.len() {
            for j in 0..block_headers[i].len() {
                block_headers[i][j].bits = block_headers[0][0].bits;
            }
        }

        for i in 1..block_headers.len() {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers[i].clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            if i == block_headers.len() - 1 {
                assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
                    .contains("Error: Incorrect target."));
            } else {
                assert!(outcome.is_success());
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_getting_an_error_if_submitting_unattached_block(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([fork_block_header_example_2()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;

        assert!(
            !outcome.is_success(),
            "Expected transaction to fail, but it succeeded"
        );

        let failure_message = format!("{:?}", outcome.failures());

        assert!(
            failure_message.contains("PrevBlockNotFound"),
            "Expected failure message to contain 'PrevBlockNotFound', but got: {failure_message}",
        );

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
}
