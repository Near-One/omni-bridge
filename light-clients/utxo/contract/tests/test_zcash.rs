#[cfg(feature = "zcash")]
mod test_zcash {
    use btc_types::contract_args::InitArgs;
    use btc_types::header::{ExtendedHeader, Header};
    use near_sdk::NearToken;
    use near_workspaces::{cargo_near_build, Account, Contract};
    use serde_json::json;
    use std::fs::File;
    use std::io::BufReader;
    use std::str::FromStr;

    const STORAGE_DEPOSIT_PER_BLOCK: NearToken = NearToken::from_millinear(500);

    async fn build_contract() -> Vec<u8> {
        let artifact = cargo_near_build::build_with_cli(cargo_near_build::BuildOpts {
            manifest_path: Some(
                cargo_near_build::camino::Utf8PathBuf::from_str("./Cargo.toml")
                    .expect("camino PathBuf from str"),
            ),
            no_default_features: true,
            features: Some("zcash".to_string()),
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("building contract: {:?}", e));

        let file = artifact.canonicalize().unwrap();
        std::fs::read(&file).unwrap()
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

    async fn init_zcash_contract() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_zcash_genesis().await?;

        // 11 blocks for MTP + 17 for the pre-NU7 averaging window
        let outcome = bootstrap(
            &contract,
            contract.as_account(),
            read_zcash_blocks()[1..29].to_vec(),
        )
        .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        Ok((contract, user_account))
    }

    async fn init_zcash_genesis() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract_wasm = build_contract().await;

        let contract = sandbox.dev_deploy(&contract_wasm).await?;

        let initial_blocks = read_zcash_blocks();
        let genesis_block = initial_blocks[0].clone();

        let args = InitArgs {
            genesis_block_hash: genesis_block.block_hash(),
            genesis_block_height: 2940821,
            skip_pow_verification: false,
            gc_threshold: 2000,
            network: btc_types::network::Network::Mainnet,
            genesis_block,
        };

        let outcome = contract
            .call("init")
            .args_json(json!({
                "args": serde_json::to_value(args).unwrap(),
            }))
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        let user_account = sandbox.dev_create_account().await?;
        grant_relayer_role(&contract, &user_account).await?;

        Ok((contract, user_account))
    }

    #[tokio::test]
    async fn test_bootstrap_blocks_is_private() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, relayer) = init_zcash_genesis().await?;

        let outcome = bootstrap(&contract, &relayer, read_zcash_blocks()[1..29].to_vec()).await?;
        assert!(
            format!("{:?}", outcome.failures()).contains("Method bootstrap_blocks is private"),
            "{:?}",
            outcome.failures()
        );

        let outcome = submit(&contract, &relayer, read_zcash_blocks()[1..2].to_vec()).await?;
        assert!(
            format!("{:?}", outcome.failures()).contains("ERR_BOOTSTRAP_IN_PROGRESS"),
            "{:?}",
            outcome.failures()
        );

        Ok(())
    }

    async fn submit(
        contract: &Contract,
        relayer: &Account,
        headers: Vec<Header>,
    ) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
        call_with_headers(contract, relayer, "submit_blocks", headers).await
    }

    async fn bootstrap(
        contract: &Contract,
        relayer: &Account,
        headers: Vec<Header>,
    ) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
        call_with_headers(contract, relayer, "bootstrap_blocks", headers).await
    }

    async fn call_with_headers(
        contract: &Contract,
        relayer: &Account,
        method: &str,
        headers: Vec<Header>,
    ) -> Result<near_workspaces::result::ExecutionFinalResult, Box<dyn std::error::Error>> {
        let num_headers = u128::try_from(headers.len()).unwrap();
        Ok(relayer
            .call(contract.id(), method)
            .args_borsh(headers)
            .deposit(STORAGE_DEPOSIT_PER_BLOCK.saturating_mul(num_headers))
            .max_gas()
            .transact()
            .await?)
    }

    fn read_zcash_blocks() -> Vec<Header> {
        let file =
            File::open("./tests/data/zcash_initial_blocks.json").expect("Unable to open file");
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).expect("Unable to parse JSON")
    }

    /// Fetches the wasm currently deployed on a mainnet account via plain RPC.
    /// (`near_workspaces::mainnet()` is not used because its client fails to
    /// parse responses from current mainnet nodes.)
    async fn fetch_mainnet_wasm(account_id: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use base64::Engine;

        let response: serde_json::Value = reqwest::Client::new()
            .post("https://rpc.mainnet.near.org")
            .json(&json!({
                "jsonrpc": "2.0",
                "id": "dontcare",
                "method": "query",
                "params": {
                    "request_type": "view_code",
                    "finality": "final",
                    "account_id": account_id,
                },
            }))
            .send()
            .await?
            .json()
            .await?;

        let code_base64 = response["result"]["code_base64"]
            .as_str()
            .unwrap_or_else(|| panic!("no code_base64 in response: {response}"));
        Ok(base64::engine::general_purpose::STANDARD.decode(code_base64)?)
    }

    /// Initializes a sandbox contract from the wasm currently deployed on
    /// mainnet (`zcash-client.bridge.near`)
    #[tokio::test]
    async fn test_migration_from_mainnet_wasm() -> Result<(), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let old_wasm = fetch_mainnet_wasm("zcash-client.bridge.near").await?;

        let contract = sandbox.dev_deploy(&old_wasm).await?;

        // The mainnet wasm (0.6.1) takes `genesis_block` in `InitArgs` and bootstraps separately.
        let initial_blocks = read_zcash_blocks();
        let outcome = contract
            .call("init")
            .args_json(json!({ "args": {
                "genesis_block_hash": initial_blocks[0].block_hash(),
                "genesis_block_height": 2940821,
                "skip_pow_verification": false,
                "gc_threshold": 2000,
                "network": btc_types::network::Network::Mainnet,
                "genesis_block": initial_blocks[0],
            }}))
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        let outcome = bootstrap(
            &contract,
            contract.as_account(),
            initial_blocks[1..29].to_vec(),
        )
        .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        // Upgrade to the current wasm.
        let new_wasm = build_contract().await;
        contract
            .as_account()
            .deploy(&new_wasm)
            .await?
            .into_result()?;

        let outcome = contract
            .call("migrate")
            .args_json(json!({}))
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        // State is intact after the migration and the contract is functional.
        let last_header = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?
            .json::<ExtendedHeader>()?;
        assert_eq!(last_header.block_header, initial_blocks[28].clone().into());

        let user_account = sandbox.dev_create_account().await?;
        grant_relayer_role(&contract, &user_account).await?;
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(initial_blocks[29..32].to_vec())
            .max_gas()
            .deposit(STORAGE_DEPOSIT_PER_BLOCK.saturating_mul(3))
            .transact()
            .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        Ok(())
    }

    #[tokio::test]
    async fn test_init() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, _user_account) = init_zcash_contract().await?;

        let outcome = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?;

        let blocks = read_zcash_blocks();
        assert_eq!(
            outcome.json::<ExtendedHeader>()?.block_header,
            blocks[28].clone().into()
        );

        Ok(())
    }

    /// Inits a testnet contract with synthetic headers and bootstraps them in
    /// relayer-sized batches. Returns the bootstrapped chain plus one more header
    /// right after it, which is the first block that gets fully checked.
    async fn init_synthetic_testnet(
        genesis_block_height: u64,
        num_bootstrap_blocks: usize,
    ) -> Result<(Contract, Account, Vec<Header>), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let contract = sandbox.dev_deploy(&build_contract().await).await?;

        let mut headers: Vec<Header> = vec![read_zcash_blocks()[0].clone()];
        for _ in 0..num_bootstrap_blocks {
            let prev = headers.last().unwrap();
            let mut header = prev.clone();
            header.prev_block_hash = prev.block_hash();
            header.time = prev.time + 25;
            headers.push(header);
        }

        let args = InitArgs {
            genesis_block_hash: headers[0].block_hash(),
            genesis_block_height,
            skip_pow_verification: false,
            gc_threshold: 2000,
            network: btc_types::network::Network::Testnet,
            genesis_block: headers[0].clone(),
        };
        let outcome = contract
            .call("init")
            .args_json(json!({ "args": serde_json::to_value(args).unwrap() }))
            .max_gas()
            .transact()
            .await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());

        let user_account = sandbox.dev_create_account().await?;
        grant_relayer_role(&contract, &user_account).await?;

        let mut max_batch_gas = 0;
        for batch in headers[1..num_bootstrap_blocks].chunks(15) {
            let outcome = bootstrap(&contract, contract.as_account(), batch.to_vec()).await?;
            assert!(outcome.is_success(), "{:?}", outcome.failures());
            max_batch_gas = max_batch_gas.max(outcome.total_gas_burnt.as_tgas());
        }
        println!("max bootstrap batch gas: {max_batch_gas} Tgas");

        let last_header = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?
            .json::<ExtendedHeader>()?;
        assert_eq!(
            last_header.block_height,
            genesis_block_height + u64::try_from(num_bootstrap_blocks).unwrap() - 1
        );

        Ok((contract, user_account, headers))
    }

    /// Submits the first fully checked block with wrong `bits`, so it fails right
    /// after reading the whole averaging window. Returns the burnt gas.
    async fn fully_checked_block_gas(
        contract: &Contract,
        relayer: &Account,
        mut header: Header,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        header.bits = 0x1d00ffff;
        let outcome = submit(contract, relayer, vec![header]).await?;
        assert!(
            format!("{:?}", outcome.failures()).contains("bad-diffbits"),
            "{:?}",
            outcome.failures()
        );
        Ok(outcome.total_gas_burnt.as_gas())
    }

    /// Bootstraps 11 + 102 blocks right before the testnet NU7 activation in
    /// relayer-sized batches; the next block is fully checked.
    #[tokio::test]
    async fn test_bootstrap_before_nu7_activation() -> Result<(), Box<dyn std::error::Error>> {
        let config = btc_types::network::get_zcash_config(btc_types::network::Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();
        let num_bootstrap_blocks = 1 + 11 + 102;

        let (contract, user_account, headers) = init_synthetic_testnet(
            activation_height - u64::try_from(num_bootstrap_blocks).unwrap(),
            num_bootstrap_blocks,
        )
        .await?;

        // The synthetic block at the activation height has no valid Equihash solution
        let outcome = submit(
            &contract,
            &user_account,
            vec![headers[num_bootstrap_blocks].clone()],
        )
        .await?;
        assert!(outcome.is_failure());

        Ok(())
    }

    /// Estimates the gas of a real post-NU7 block: a real pre-NU7 block (17-block
    /// window, real Equihash) plus the cost of reading 85 more window headers.
    #[tokio::test]
    async fn test_check_pow_gas_after_nu7() -> Result<(), Box<dyn std::error::Error>> {
        let config = btc_types::network::get_zcash_config(btc_types::network::Network::Testnet);
        let activation_height = config.nu7_activation_height.unwrap();

        let (contract, relayer, headers) =
            init_synthetic_testnet(activation_height - 200, 1 + 11 + 17).await?;
        let pre_nu7_window_gas =
            fully_checked_block_gas(&contract, &relayer, headers[29].clone()).await?;

        let (contract, relayer, headers) =
            init_synthetic_testnet(activation_height - 114, 1 + 11 + 102).await?;
        let post_nu7_window_gas =
            fully_checked_block_gas(&contract, &relayer, headers[114].clone()).await?;

        let (contract, relayer) = init_zcash_contract().await?;
        let blocks = read_zcash_blocks();
        let outcome = submit(&contract, &relayer, blocks[29..30].to_vec()).await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());
        let real_one_block_gas = outcome.total_gas_burnt.as_gas();
        let outcome = submit(&contract, &relayer, blocks[30..32].to_vec()).await?;
        assert!(outcome.is_success(), "{:?}", outcome.failures());
        let real_two_blocks_gas = outcome.total_gas_burnt.as_gas();

        let real_per_block_gas = real_two_blocks_gas - real_one_block_gas;
        let extra_window_gas = post_nu7_window_gas - pre_nu7_window_gas;
        let tgas = |gas: u64| gas as f64 / 1e12;
        println!(
            "window 17 failed block: {:.2} Tgas, window 102 failed block: {:.2} Tgas",
            tgas(pre_nu7_window_gas),
            tgas(post_nu7_window_gas)
        );
        println!(
            "real pre-NU7: 1 block {:.2} Tgas, 2 blocks {:.2} Tgas, per block {:.2} Tgas",
            tgas(real_one_block_gas),
            tgas(real_two_blocks_gas),
            tgas(real_per_block_gas)
        );
        println!(
            "estimated real post-NU7 per block: {:.2} Tgas, fixed cost: {:.2} Tgas",
            tgas(real_per_block_gas + extra_window_gas),
            tgas(real_one_block_gas - real_per_block_gas)
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_block_submission() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_zcash_contract().await?;

        let blocks = read_zcash_blocks();

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(blocks[29..32].to_vec())
            .max_gas()
            .deposit(STORAGE_DEPOSIT_PER_BLOCK.saturating_mul(3))
            .transact()
            .await?;

        assert!(outcome.is_success());

        let last_header = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?
            .json::<ExtendedHeader>()?;
        assert_eq!(last_header.block_header, blocks[31].clone().into());

        assert_eq!(last_header.block_height, 2940852);

        Ok(())
    }

    #[tokio::test]
    async fn test_block_submission_out_of_order() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_zcash_contract().await?;

        let blocks = read_zcash_blocks();

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([blocks[30].clone()].to_vec())
            .max_gas()
            .deposit(STORAGE_DEPOSIT_PER_BLOCK.saturating_mul(3))
            .transact()
            .await?;

        assert!(outcome.is_failure());

        assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
            .contains("PrevBlockNotFound"));

        Ok(())
    }

    #[tokio::test]
    async fn test_block_submission_invalid_target() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_zcash_contract().await?;

        let blocks = read_zcash_blocks();
        let mut invalid_block = blocks[29].clone();
        invalid_block.bits += 1;

        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh([invalid_block].to_vec())
            .max_gas()
            .deposit(STORAGE_DEPOSIT_PER_BLOCK.saturating_mul(3))
            .transact()
            .await?;

        assert!(outcome.is_failure());

        assert!(format!("{:?}", outcome.failures()[0].clone().into_result())
            .contains("bad-diffbits: incorrect proof of work"));

        Ok(())
    }
}
