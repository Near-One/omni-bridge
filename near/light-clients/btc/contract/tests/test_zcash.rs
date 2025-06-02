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

    async fn init_zcash_contract() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
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
            submit_blocks: initial_blocks[..29].to_vec(),
        };

        let outcome = contract
            .call("init")
            .args_json(json!({
                "args": serde_json::to_value(args).unwrap(),
            }))
            .max_gas()
            .transact()
            .await?;

        println!("outcome: {:?}", outcome);

        assert!(outcome.is_success());

        let user_account = sandbox.dev_create_account().await?;

        Ok((contract, user_account))
    }

    fn read_zcash_blocks() -> Vec<Header> {
        let file =
            File::open("./tests/data/zcash_initial_blocks.json").expect("Unable to open file");
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).expect("Unable to parse JSON")
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
            .contains("Error: Incorrect target."));

        Ok(())
    }
}
