#[cfg(feature = "bitcoin")]
mod test_basics {
    use btc_types::contract_args::{InitArgs, ProofArgs, ProofArgsV2};
    use btc_types::hash::H256;
    use btc_types::header::{ExtendedHeader, Header};
    use near_sdk::NearToken;
    use near_workspaces::{Account, Contract};
    use serde_json::json;
    use std::fs::File;
    use std::io::BufReader;

    const STORAGE_DEPOSIT_PER_BLOCK: NearToken = NearToken::from_millinear(500);

    // 12-block init list: genesis + 11 fake blocks branching from genesis with
    // bits=0x207FFFFF (near-zero work). This satisfies the MEDIAN_TIME_SPAN+1
    // requirement while keeping genesis at height 0. Blocks submitted after init
    // with normal bits (e.g. 486_604_799) have enough chainwork to be promoted
    // over the fake mainchain tip.
    fn make_init_submit_blocks() -> Vec<Header> {
        let genesis = genesis_block_header();
        let genesis_hash = genesis.block_hash().to_string();
        let mut blocks = vec![genesis];
        for i in 0u32..11 {
            let fake: Header = serde_json::from_value(serde_json::json!({
                "version": 1,
                "prev_block_hash": genesis_hash,
                "merkle_root": "0000000000000000000000000000000000000000000000000000000000000000",
                "time": 1_231_006_506u32 + i,
                "bits": 0x207fffffu32,
                "nonce": i,
            }))
            .unwrap();
            blocks.push(fake);
        }
        blocks
    }

    /// Grant the `UnrestrictedSubmitBlocks` role to an account so it passes the
    /// `#[trusted_relayer]` guard on `submit_blocks`. The contract itself is the
    /// super admin (set during `init`), so it can grant any role.
    async fn grant_relayer_role(
        contract: &Contract,
        account: &Account,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let outcome = contract
            .call("acl_grant_role")
            .args_json(json!({
                "role": "UnrestrictedSubmitBlocks",
                "account_id": account.id(),
            }))
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Failed to grant role: {:?}",
            outcome.failures()
        );
        Ok(())
    }

    async fn init_contract() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = near_workspaces::compile_project("./").await?;

        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let submit_blocks = make_init_submit_blocks();
        let args = InitArgs {
            genesis_block_hash: submit_blocks[0].block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: true,
            gc_threshold: 20,
            network: btc_types::network::Network::Mainnet,
            submit_blocks,
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
        grant_relayer_role(&contract, &user_account).await?;

        Ok((contract, user_account))
    }

    async fn init_contract_from_file(
        gc_threshold: u64,
    ) -> Result<(Contract, Account, Vec<Vec<Header>>), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = near_workspaces::compile_project("./").await?;

        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let all_block_headers =
            read_blocks_from_json("./tests/data/blocks_headers_685440-687456_mainnet.json");

        // Init with 12 blocks (685440-685451) so that MTP can be computed for the
        // first submitted block (needs 11 ancestors in storage).
        // Layout in JSON: batch[0]=[685440], batch[1]=[685441-685446], batch[2][0..5]=[685447-685451]
        let mut init_blocks: Vec<Header> = Vec::new();
        init_blocks.push(all_block_headers[0][0].clone());
        init_blocks.extend_from_slice(&all_block_headers[1]);
        init_blocks.extend_from_slice(&all_block_headers[2][..5]);
        assert_eq!(init_blocks.len(), 12);

        let args = InitArgs {
            genesis_block_hash: init_blocks[0].block_hash(),
            genesis_block_height: 685_440,
            skip_pow_verification: false,
            gc_threshold,
            network: btc_types::network::Network::Mainnet,
            submit_blocks: init_blocks,
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
        grant_relayer_role(&contract, &user_account).await?;

        // Return blocks NOT yet submitted (batch[2][5..] onward).
        let remaining = remaining_after_init(&all_block_headers);
        Ok((contract, user_account, remaining))
    }

    // Returns the blocks from the JSON that are not included in the 12-block init.
    // The first 12 blocks are: batch[0] (1) + batch[1] (6) + batch[2][0..5] (5).
    fn remaining_after_init(all_headers: &[Vec<Header>]) -> Vec<Vec<Header>> {
        let mut result = Vec::new();
        if all_headers[2].len() > 5 {
            result.push(all_headers[2][5..].to_vec());
        }
        result.extend_from_slice(&all_headers[3..]);
        result
    }

    #[tokio::test]
    async fn test_setting_genesis_block() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, _user_account) = init_contract().await?;

        // init provides genesis + 11 fake blocks; verify genesis is recorded at height 0
        let outcome = contract
            .view("get_block_hash_by_height")
            .args_json(json!({"height": 0}))
            .await?;

        assert_eq!(
            outcome.json::<Option<H256>>()?,
            Some(genesis_block_header().block_hash())
        );

        Ok(())
    }

    /// Build three test blocks branching from fake_0 (the init mainchain tip):
    ///   - main_block: extends fake_0 on mainchain (height 2)
    ///   - fork_1:     extends fake_0 as fork      (height 2, same chainwork as main_block)
    ///   - fork_2:     extends fork_1               (height 3, higher chainwork → triggers reorg)
    fn make_reorg_test_blocks() -> (Header, Header, Header) {
        let init_blocks = make_init_submit_blocks();
        let fake_0_hash = init_blocks[1].block_hash().to_string();

        let main_block: Header = serde_json::from_value(json!({
            "version": 1,
            "prev_block_hash": fake_0_hash,
            "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "time": 1_231_006_510,
            "bits": 486_604_799,
            "nonce": 2_083_236_893_u32,
        }))
        .unwrap();

        let fork_1: Header = serde_json::from_value(json!({
            "version": 1,
            "prev_block_hash": fake_0_hash,
            "merkle_root": "0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098",
            "time": 1_231_469_665,
            "nonce": 2_573_394_689_u32,
            "bits": 486_604_799,
        }))
        .unwrap();

        let fork_1_hash = fork_1.block_hash().to_string();
        let fork_2: Header = serde_json::from_value(json!({
            "version": 1,
            "prev_block_hash": fork_1_hash,
            "merkle_root": "9b0fc92260312ce44e74ef369f5c66bbb85848f2eddd5a7a1cde251e54ccfdd5",
            "time": 1_231_469_744,
            "nonce": 1_639_830_024_u32,
            "bits": 486_604_799,
        }))
        .unwrap();

        (main_block, fork_1, fork_2)
    }

    #[tokio::test]
    async fn test_setting_chain_reorg() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (main_block, fork_1, fork_2) = make_reorg_test_blocks();

        let storage_usage_init = contract.view_account().await.unwrap().storage_usage;

        // main_block extends fake_0 (current tip) → goes to mainchain at height 2
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([main_block].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_one_block = contract.view_account().await.unwrap().storage_usage;

        // fork_1 also extends fake_0 but as a fork (same chainwork → not promoted)
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([fork_1].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_fork = contract.view_account().await.unwrap().storage_usage;

        // fork_2 extends fork_1 (higher chainwork → reorg, becomes new tip)
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([fork_2.clone()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        let storage_usage_after = contract.view_account().await.unwrap().storage_usage;
        // Reorg removes main_block from storage (replaced by fork_1 at height 2).
        // delta_reorg = mainchain map overhead only (pool nets to zero: +fork_2, −main_block).
        // delta_one  = pool entry + mainchain map overhead.
        // delta_fork = pool entry only.
        // Therefore: delta_reorg == delta_one − delta_fork.
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
            fork_2.into_light()
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
                merkle_proof: vec![merkle_tools::H256::default()],
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

        for block_headers_batch in &block_headers[..] {
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

        // Submit remaining[0] (85 blocks). Together with the 12 in init = 97 total.
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[0].clone())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success());

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

        // 12 blocks already loaded in init; submit remaining[0] (85 blocks) = 97 total.
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[0].clone())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success());
        assert_eq!(12 + block_headers[0].len(), 97);

        let outcome = contract
            .view("get_mainchain_size")
            .args_json(json!({}))
            .await?;

        // After submitting 85 blocks in one call, GC removes 85 (= batch_size), leaving 12.
        // The explicit run_mainchain_gc(100) below will then bring it down to 10.
        assert_eq!(outcome.json::<u64>().unwrap(), 12);

        let outcome = contract
            .view("get_last_n_blocks_hashes")
            .args_json(json!({"skip": 0, "limit": 100}))
            .await?;

        let mainchain_blocks = outcome.json::<Vec<H256>>().unwrap();
        assert_eq!(mainchain_blocks.len(), 12);
        for i in 0..mainchain_blocks.len() {
            assert_eq!(
                mainchain_blocks[mainchain_blocks.len() - i - 1],
                block_headers[0][block_headers[0].len() - i - 1].block_hash()
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
        // gc_threshold=200: init (12 blocks) is well below threshold, so the first few
        // batches require deposit. After 3 batches with deposit (~12+85+85+85=267 total),
        // GC kicks in and subsequent batches can be submitted for free.
        let (contract, user_account, block_headers) = init_contract_from_file(200).await?;

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(block_headers[0].clone())
            .max_gas()
            .transact()
            .await?;

        assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
            .contains("Required deposit"));

        for block_headers_batch in block_headers.iter().take(3) {
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

        for i in 0..block_headers.len() {
            let outcome = user_account
                .call(contract.id(), "submit_blocks")
                .args_borsh(block_headers[i].clone())
                .deposit(STORAGE_DEPOSIT_PER_BLOCK)
                .max_gas()
                .transact()
                .await?;

            if i == block_headers.len() - 1 {
                assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
                    .contains("bad-diffbits: incorrect proof of work"));
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

    /// A random account without any roles must be rejected by the
    /// `#[trusted_relayer]` guard when calling `submit_blocks`.
    #[tokio::test]
    async fn test_unauthorized_account_cannot_submit_blocks(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = near_workspaces::compile_project("./").await?;
        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let submit_blocks = make_init_submit_blocks();
        let args = InitArgs {
            genesis_block_hash: submit_blocks[0].block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: true,
            gc_threshold: 20,
            network: btc_types::network::Network::Mainnet,
            submit_blocks,
        };
        let outcome = contract
            .call("init")
            .args_json(json!({
                "args": serde_json::to_value(args).unwrap(),
            }))
            .transact()
            .await?;
        assert!(outcome.is_success());

        // Create an account but do NOT grant any role.
        let unauthorized_account = sandbox.dev_create_account().await?;

        let init_blocks = make_init_submit_blocks();
        let fake_0_hash = init_blocks[1].block_hash().to_string();
        let block: Header = serde_json::from_value(json!({
            "version": 1,
            "prev_block_hash": fake_0_hash,
            "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "time": 1_231_006_510,
            "bits": 486_604_799,
            "nonce": 2_083_236_893_u32,
        }))
        .unwrap();

        let outcome = unauthorized_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([block].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;

        assert!(
            !outcome.is_success(),
            "Expected submit_blocks to fail for an account without roles, but it succeeded"
        );

        let failure_message = format!("{:?}", outcome.failures());
        assert!(
            failure_message.contains("Relayer is not active"),
            "Expected failure message to contain 'Relayer is not active', but got: {failure_message}",
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

    /// Helper: creates a custom block with a 2-tx merkle tree and submits it to the contract.
    /// Returns (block_header, coinbase_hash, tx_hash).
    ///
    /// Tree structure:
    ///   merkle_root = double_sha256(coinbase_hash || tx_hash)
    ///   coinbase at index 0, tx at index 1
    async fn submit_two_tx_block(
        contract: &Contract,
        user_account: &Account,
    ) -> Result<(Header, H256, H256), Box<dyn std::error::Error>> {
        let coinbase_hash: H256 =
            "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
                .parse()
                .unwrap();
        let tx_hash: H256 = "0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098"
            .parse()
            .unwrap();

        let mut concat = Vec::with_capacity(64);
        concat.extend(coinbase_hash.0);
        concat.extend(tx_hash.0);
        let merkle_root: H256 = btc_types::hash::double_sha256(&concat);

        let block = Header {
            version: 1,
            prev_block_hash: "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f"
                .parse()
                .unwrap(),
            merkle_root,
            time: 1_231_006_506,
            bits: 486_604_799,
            nonce: 2_083_236_893,
        };

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([block.clone()].to_vec())
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .transact()
            .await?;
        assert!(outcome.is_success());

        Ok((block, coinbase_hash, tx_hash))
    }

    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_valid() -> Result<(), Box<dyn std::error::Error>>
    {
        let (contract, user_account) = init_contract().await?;
        let (block, coinbase_hash, tx_hash) = submit_two_tx_block(&contract, &user_account).await?;

        // Verify tx at index 1; coinbase proof for coinbase at index 0.
        // Both proofs have depth 1: sibling is the other tx.
        let result: bool = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: tx_hash.clone(),
                tx_block_blockhash: block.block_hash(),
                tx_index: 1,
                merkle_proof: vec![coinbase_hash.clone()],
                coinbase_tx_id: coinbase_hash,
                coinbase_merkle_proof: vec![tx_hash],
                confirmations: 0,
            })
            .await?
            .json()?;

        assert!(result, "Valid transaction inclusion should return true");

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_invalid_coinbase_proof(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (block, _coinbase_hash, tx_hash) =
            submit_two_tx_block(&contract, &user_account).await?;

        // Wrong coinbase_tx_id -> coinbase proof won't match merkle root
        let result = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: tx_hash.clone(),
                tx_block_blockhash: block.block_hash(),
                tx_index: 1,
                merkle_proof: vec![H256::default()],
                coinbase_tx_id: H256::default(),
                coinbase_merkle_proof: vec![tx_hash],
                confirmations: 0,
            })
            .await;

        assert!(
            result.is_err(),
            "Should fail when coinbase merkle proof is incorrect"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_mismatched_proof_lengths(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (block, coinbase_hash, tx_hash) = submit_two_tx_block(&contract, &user_account).await?;

        // merkle_proof has 1 element, coinbase_merkle_proof has 0 — lengths don't match
        let result = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: tx_hash,
                tx_block_blockhash: block.block_hash(),
                tx_index: 1,
                merkle_proof: vec![coinbase_hash.clone()],
                coinbase_tx_id: coinbase_hash,
                coinbase_merkle_proof: vec![],
                confirmations: 0,
            })
            .await;

        assert!(
            result.is_err(),
            "Should fail when merkle proof and coinbase merkle proof have different lengths"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_valid_coinbase_invalid_tx_proof(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (block, coinbase_hash, tx_hash) = submit_two_tx_block(&contract, &user_account).await?;

        // Coinbase proof is valid, but tx proof uses wrong tx_id -> returns false
        let result: bool = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: H256::default(),
                tx_block_blockhash: block.block_hash(),
                tx_index: 1,
                merkle_proof: vec![coinbase_hash.clone()],
                coinbase_tx_id: coinbase_hash,
                coinbase_merkle_proof: vec![tx_hash],
                confirmations: 0,
            })
            .await?
            .json()?;

        assert!(
            !result,
            "Should return false when coinbase proof is valid but transaction proof is not"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_block_not_found(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;

        let result = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: H256::default(),
                tx_block_blockhash: H256::default(),
                tx_index: 0,
                merkle_proof: vec![H256::default()],
                coinbase_tx_id: H256::default(),
                coinbase_merkle_proof: vec![H256::default()],
                confirmations: 0,
            })
            .await;

        assert!(
            result.is_err(),
            "Should fail when block is not found in headers pool"
        );

        Ok(())
    }

    /// Verifying the coinbase as if it were the target tx itself: `tx_id == coinbase_tx_id`,
    /// `tx_index == 0`, and both merkle proofs identical. This is a degenerate-but-valid
    /// case — v2 must still accept it.
    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_coinbase_self(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (block, coinbase_hash, tx_hash) = submit_two_tx_block(&contract, &user_account).await?;

        let result: bool = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: coinbase_hash.clone(),
                tx_block_blockhash: block.block_hash(),
                tx_index: 0,
                merkle_proof: vec![tx_hash.clone()],
                coinbase_tx_id: coinbase_hash,
                coinbase_merkle_proof: vec![tx_hash],
                confirmations: 0,
            })
            .await?
            .json()?;

        assert!(
            result,
            "Verifying the coinbase itself (tx_id == coinbase_tx_id, both proofs equal) \
             should return true"
        );

        Ok(())
    }

    /// The test block is at the chain tip after `submit_two_tx_block`, so only 1
    /// confirmation is available. Asking for 2 must trigger the
    /// "Not enough blocks confirmed" require in `verify_transaction_inclusion`.
    #[tokio::test]
    async fn test_verify_transaction_inclusion_v2_insufficient_confirmations(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_contract().await?;
        let (block, coinbase_hash, tx_hash) = submit_two_tx_block(&contract, &user_account).await?;

        let result = user_account
            .view(contract.id(), "verify_transaction_inclusion_v2")
            .args_borsh(ProofArgsV2 {
                tx_id: tx_hash.clone(),
                tx_block_blockhash: block.block_hash(),
                tx_index: 1,
                merkle_proof: vec![coinbase_hash.clone()],
                coinbase_tx_id: coinbase_hash,
                coinbase_merkle_proof: vec![tx_hash],
                confirmations: 2,
            })
            .await;

        assert!(
            result.is_err(),
            "Should fail when fewer confirmations are available than requested"
        );

        Ok(())
    }
}
