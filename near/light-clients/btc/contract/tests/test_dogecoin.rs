#[cfg(feature = "dogecoin")]
mod test_dogecoin {
    use btc_types::aux::AuxData;
    use btc_types::contract_args::InitArgs;
    use btc_types::hash::H256;
    use btc_types::header::Header;
    use btc_types::network::Network;
    use near_sdk::NearToken;
    use near_workspaces::{Account, Contract};
    use serde_json::json;

    const STORAGE_DEPOSIT_PER_BLOCK: NearToken = NearToken::from_millinear(500);
    const DOGE_BITS: u32 = 0x1e0fffff;

    fn doge_block_wrong_bits() -> Header {
        let init_blocks = doge_init_blocks();
        let last = init_blocks.last().unwrap();
        Header {
            version: 1,
            prev_block_hash: last.block_hash(),
            merkle_root: H256::default(),
            time: last.time + 60,
            bits: DOGE_BITS - 1,
            nonce: 0,
        }
    }

    async fn compile_dogecoin_wasm() -> Vec<u8> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let status = tokio::process::Command::new("cargo")
            .args([
                "near",
                "build",
                "non-reproducible-wasm",
                "--no-default-features",
                "--features",
                "dogecoin",
            ])
            .current_dir(manifest_dir)
            .status()
            .await
            .expect("Failed to run cargo near build for dogecoin WASM");
        assert!(status.success(), "Failed to build dogecoin WASM");

        let wasm_path = format!("{manifest_dir}/target/near/btc_light_client_contract.wasm");
        tokio::fs::read(&wasm_path)
            .await
            .unwrap_or_else(|e| panic!("Failed to read dogecoin WASM at {wasm_path}: {e}"))
    }

    // Build a chain of synthetic blocks long enough for MTP (need >= 12).
    // Genesis has DOGE_BITS, each subsequent block steps time by 60 seconds.
    fn doge_init_blocks() -> Vec<Header> {
        let mut blocks = Vec::new();
        let mut prev_hash = H256::default();
        let mut time = 1_500_000_000u32;
        for _ in 0..12 {
            let h = Header {
                version: 1,
                prev_block_hash: prev_hash,
                merkle_root: H256::default(),
                time,
                bits: DOGE_BITS,
                nonce: 0,
            };
            prev_hash = h.block_hash();
            time += 60;
            blocks.push(h);
        }
        blocks
    }

    async fn init_dogecoin_contract() -> Result<(Contract, Account), Box<dyn std::error::Error>> {
        let sandbox = near_workspaces::sandbox().await?;
        let wasm = compile_dogecoin_wasm().await;
        let contract = sandbox.dev_deploy(&wasm).await?;

        let init_blocks = doge_init_blocks();
        let genesis = init_blocks[0].clone();
        let args = InitArgs {
            genesis_block_hash: genesis.block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: false,
            gc_threshold: 20,
            network: Network::Mainnet,
            submit_blocks: init_blocks,
        };

        let outcome = contract
            .call("init")
            .args_json(json!({ "args": serde_json::to_value(args).unwrap() }))
            .max_gas()
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Init failed: {:?}",
            outcome.failures()
        );

        let user_account = sandbox.dev_create_account().await?;
        Ok((contract, user_account))
    }

    // ---------------------------------------------------------------------------
    // Real mainnet blocks 5_800_000–5_800_013 (2025-07-20).
    // Source: BlockCypher Dogecoin API — https://api.blockcypher.com/v1/doge/main/blocks/<height>
    // Chain linkage verified: each prev_block_hash equals the hash of the preceding block.
    // nonce=0 is expected for AuxPoW blocks (actual nonce is in the coinbase tx).
    // version=6422788 (0x620104): chain_id=0x62 (Dogecoin), AuxPoW flag (bit 8) set.
    // ---------------------------------------------------------------------------

    fn make_block(prev_block_hash: &str, merkle_root: &str, time: u32, bits: u32) -> Header {
        serde_json::from_value(json!({
            "version": 6422788,
            "prev_block_hash": prev_block_hash,
            "merkle_root": merkle_root,
            "time": time,
            "bits": bits,
            "nonce": 0u32
        }))
        .unwrap()
    }

    // Returns blocks 5_800_000..=5_800_013 in order.
    fn mainnet_blocks() -> Vec<Header> {
        vec![
            // 5_800_000  hash: 8d1540c92ec87451d73573fec3720ca7e835e630538096e3e11c56dec8205e2e
            make_block(
                "fbf1d4af85612424aa08c4b14df869aca3fe1518446d4e675595bf83b9766c87",
                "23038370556fd5b6d5881001aa748dd53129e3901ebffafc9b7d82df41928650",
                1753007429,
                436262933,
            ),
            // 5_800_001  hash: afa3f83d1afbbd2ed26efbafc57ea5410e790874120915d06548256094c8a609
            make_block(
                "8d1540c92ec87451d73573fec3720ca7e835e630538096e3e11c56dec8205e2e",
                "5d88372c5341db5591d678e669fc539316bf550f176a80368b4359c68b96cf86",
                1753007473,
                436261089,
            ),
            // 5_800_002  hash: 0f34eb5e729a97c8fd557ea5a128c932e056c21e7edb8ef36c4537ca70e5d48c
            make_block(
                "afa3f83d1afbbd2ed26efbafc57ea5410e790874120915d06548256094c8a609",
                "b0e867993a3a57d17918b0aac81bcdd5268faf76dc8c3a79d64ce124f2b9178d",
                1753007499,
                436259306,
            ),
            // 5_800_003  hash: ccdf48cde084529f9099b054983060c3cdb87eb5ff8c344cab355aae0e736cfd
            make_block(
                "0f34eb5e729a97c8fd557ea5a128c932e056c21e7edb8ef36c4537ca70e5d48c",
                "205e3576dab3c3f88d7e3a92fa61c46fe36e9e382a5a3884ac1c21cd0fba60e4",
                1753007552,
                436255860,
            ),
            // 5_800_004  hash: 49cfcda2406983505c19970999a506b54008373ce27d2743fd0968ac0ffb4c0c
            make_block(
                "ccdf48cde084529f9099b054983060c3cdb87eb5ff8c344cab355aae0e736cfd",
                "ad279b894d947eb2d11d37528e0245b2a3a93412deebd45b789ab727a67fc197",
                1753007561,
                436255860,
            ),
            // 5_800_005  hash: 8dd2b088ebb3a8e9529935beed8db00cfbcc71440ca8d3ada9d3e68187c63170
            make_block(
                "49cfcda2406983505c19970999a506b54008373ce27d2743fd0968ac0ffb4c0c",
                "1dc7bc810faa6122c8f8459bf9d3928a87a0e1777902c34e3912d1c87e9d3ad0",
                1753007611,
                436251035,
            ),
            // 5_800_006  hash: dd8dd477409b51a9e046270a04d762c999ae1e65c707ff20acce0d1787180ac4
            make_block(
                "8dd2b088ebb3a8e9529935beed8db00cfbcc71440ca8d3ada9d3e68187c63170",
                "18dc598772ffa0ecf5b6467e3ebda5d3c8f3580bda413778df701c810666c997",
                1753007671,
                436250311,
            ),
            // 5_800_007  hash: 646865ff3b66d65e0e7824904873236d970821495d1bf085b260f3491961b2b8
            make_block(
                "dd8dd477409b51a9e046270a04d762c999ae1e65c707ff20acce0d1787180ac4",
                "7944974d1084400707550887e1c122aa6c73cff44a26998f261320408094e64d",
                1753007730,
                436250311,
            ),
            // 5_800_008  hash: 93bdeeafdc34aed94d6ababbbe3c9af8f7609ab0141e1d462bcc7aed3f50eaab
            make_block(
                "646865ff3b66d65e0e7824904873236d970821495d1bf085b260f3491961b2b8",
                "46317b3c4a87bdbd52b2167666c81fed94a5f717a0ee256e8b433283632ff5ec",
                1753007882,
                436250311,
            ),
            // 5_800_009  hash: 2784b47216f76bb35b0f31bba70bb8bb7db771228ed46d09893755d9317733f4
            make_block(
                "93bdeeafdc34aed94d6ababbbe3c9af8f7609ab0141e1d462bcc7aed3f50eaab",
                "9f59d1aaa4220fd8be905646a8824204b0c1842b8842ff9867767bc409576efe",
                1753007889,
                436258138,
            ),
            // 5_800_010  hash: b2b36ed6153b789ea70518cebbd6ef90c7056cb209a38de05cd3342d1bc96942
            make_block(
                "2784b47216f76bb35b0f31bba70bb8bb7db771228ed46d09893755d9317733f4",
                "51ffd63f21a101ca906f5d01f2cadd176975610feabc071d6216984711216e00",
                1753007899,
                436253085,
            ),
            // 5_800_011  hash: 18958cb77ab6e736afb871bec28cb9d734f720367a007185e650fb5646d46170
            make_block(
                "b2b36ed6153b789ea70518cebbd6ef90c7056cb209a38de05cd3342d1bc96942",
                "ba4deb0bebcdb1b81b928ef798899b1f17868281af8350c310886880ab3eca65",
                1753007955,
                436248538,
            ),
            // 5_800_012  hash: c6dbc782fa5a3c89dbf5ff62305a3c2890000771cc2553f6c054b422d458d74d
            make_block(
                "18958cb77ab6e736afb871bec28cb9d734f720367a007185e650fb5646d46170",
                "edd234664db51bdf968b575e965fb672c21d6d48338d949547e2734d57fb4001",
                1753008034,
                436248538,
            ),
            // 5_800_013  hash: 919970f385fa583800f89d8b84249b98a1cb73ef732ea30d5fa176ffcbf88e18
            make_block(
                "c6dbc782fa5a3c89dbf5ff62305a3c2890000771cc2553f6c054b422d458d74d",
                "37cf538c3571c620657e468dcc88598222c08b4e101cc010d5f3a87818004015",
                1753008073,
                436249902,
            ),
        ]
    }

    /// AuxPoW data for block 5_800_013, extracted from the raw Dogecoin block hex
    /// (Blockchair API) and cross-verified in Python:
    ///   - chain_root computed from DOGE block hash + chain_id=25 + chain_merkle_proof
    ///     matches the `fabe6d6d` entry in the coinbase script ✓
    ///   - coinbase_tx_hash + merkle_proof reproduces the Litecoin block's merkle_root ✓
    ///   - get_expected_index(nonce=0x096aadf9, chain_id=0x62, height=5) == 25 ✓
    ///   - Litecoin bits 0x19309265 satisfies the easier DOGE target 0x1a00a52e ✓
    fn build_aux_data_5800013() -> AuxData {
        fn h(hex: &str) -> H256 {
            serde_json::from_value(serde_json::json!(hex)).unwrap()
        }
        AuxData {
            // Parent (Litecoin block 2935420) coinbase transaction, 219 bytes.
            // Script contains merged-mining marker fabe6d6d + chain_root + n_size=32 + n_nonce.
            coinbase_tx: hex::decode(concat!(
                "010000000100000000000000000000000000000000000000000000000000",
                "00000000000000ffffffff57037cca2c2530304861736853706163653030",
                "00000000e375651dde1b6d6200047525120f0000000000002cfabe6d6d6c",
                "ff50d08b209ee04f1fdfc369595c9ab75a6d738feed0d70394dbf7b27d11",
                "e820000000f9ad6a09ffffffff02cbc75525000000001976a914448e79e5",
                "4a421c38ef3d1a0d149171761f98cdd288ac0000000000000000266a24aa",
                "21a9edfcaff70cf3c321829c6ad57ce2a32b76d81fcbf055949a1826b503",
                "6547a00ec700000000",
            ))
            .unwrap(),
            // 9-branch proof: coinbase txid → Litecoin block merkle_root.
            merkle_proof: vec![
                h("09d1f49f9953dd1dfc107cb6afb32006864822cc4961be8bd0d2f5bd3b1b1192"),
                h("88fa4b64dde919f9662b54a53e01d7560a89e326f9c2d02fe6b6ba959b0bc58f"),
                h("fa31d5748da4cee0d8a7b2ea97d6c35f96c2c5af28381bd172fe4ed67e3d09ad"),
                h("e9b23107725a652f66eb291e76490c7157d7da4b3c747e04b41485dc480d1746"),
                h("d2beb612643fecdb4e9f44529499098f7e25b4f263caff8fc8c7ee54df76b56d"),
                h("221e30b68b9b7e10acaea8216b5486ff9452ac5267560fee251c75d0d388551f"),
                h("b6dcec9457e444c158bcfdc9aa722a2989bf15dacaad79335b8cbedaa11cbfea"),
                h("886292aad6a438d026b4a168b50eb45b99721fc4264c5b0eecdac8e4ff1c4305"),
                h("b70a42669117f5c1e5a5caf5a41e06bc66f9b47c3e28de1fba4d032efa24dc9f"),
            ],
            // 5-branch proof: DOGE block hash at position 25 → chain_root in coinbase script.
            chain_merkle_proof: vec![
                h("0000000000000000000000000000000000000000000000000000000000000000"),
                h("f98c4e9736d8eb8bb46299798906695c755369a3df99a93ffdded1713f1cf6e2"),
                h("65dcfa0244a33ed04938e0d7bca472294cc0f7c5454769cfc03ce6819b402a87"),
                h("77253ca5e7538607f7c83cdd53f5f39f869bb7055ff8aa5812ef7f372bde3ed7"),
                h("4ece343f0dc46ecb527a07971591bf9af69f1841d7debd7826ac177c994bb569"),
            ],
            chain_id: 25,
            // Litecoin block 2935420 header (version=0x20000000, scrypt PoW, bits=0x19309265).
            parent_block: serde_json::from_value(json!({
                "version": 536870912i32,
                "prev_block_hash": "0d1db79cc92ff24f2657fb11b23e3c78c4a4743a9a77cb559bce5eae28344674",
                "merkle_root": "049c025705d4ceb6e3847712ebfa23762580fbc8b6915abaaa3243d1bc498d68",
                "time": 1753008193u32,
                "bits": 422613605u32,
                "nonce": 1225295933u32,
            }))
            .unwrap(),
        }
    }

    /// Submit a real 2025 Dogecoin mainnet block with full PoW + AuxPoW verification.
    ///
    /// Blocks 5_800_000–5_800_012 are pre-loaded during init (the init code always
    /// uses skip_pow=true for those). Block 5_800_013 is then submitted with
    /// skip_pow=false (from contract state), exercising the full check_pow path
    /// (bits + MTP + future-time) and the full check_aux path (AuxPoW validation).
    /// The 13 init blocks are enough for get_median_time_past (needs 12 blocks back).
    #[tokio::test]
    async fn test_real_block_submission_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        use btc_types::header::ExtendedHeader;

        let sandbox = near_workspaces::sandbox().await?;
        let wasm = compile_dogecoin_wasm().await;
        let contract = sandbox.dev_deploy(&wasm).await?;

        let mut blocks = mainnet_blocks();
        let test_block = blocks.pop().unwrap(); // 5_800_013
        let genesis = blocks[0].clone();

        let args = InitArgs {
            genesis_block_hash: genesis.block_hash(),
            genesis_block_height: 5_800_000,
            skip_pow_verification: false, // subsequent submit_blocks will do full verification
            gc_threshold: 20,
            network: Network::Mainnet,
            submit_blocks: blocks, // 5_800_000 .. 5_800_012 (13 blocks, skip=true in init loop)
        };

        let outcome = contract
            .call("init")
            .args_json(json!({ "args": serde_json::to_value(args).unwrap() }))
            .max_gas()
            .transact()
            .await?;
        assert!(
            outcome.is_success(),
            "Init failed: {:?}",
            outcome.failures()
        );

        let user_account = sandbox.dev_create_account().await?;

        // Submit block 5_800_013 with full PoW + AuxPoW verification.
        let aux_data = build_aux_data_5800013();
        let headers: Vec<(Header, Option<AuxData>)> = vec![(test_block.clone(), Some(aux_data))];
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(headers)
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;

        assert!(
            outcome.is_success(),
            "Block submission failed: {:?}",
            outcome.failures()
        );

        let last = contract
            .view("get_last_block_header")
            .args_json(json!({}))
            .await?
            .json::<ExtendedHeader>()?;

        assert_eq!(last.block_header, test_block);
        assert_eq!(last.block_height, 5_800_013);

        Ok(())
    }

    /// Submit a Dogecoin block with wrong target bits and no AuxPoW data.
    /// Expected: rejected with "Error: Incorrect target." before any AuxPoW checks.
    #[tokio::test]
    async fn test_wrong_target_no_auxpow_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_dogecoin_contract().await?;

        let headers: Vec<(Header, Option<AuxData>)> = vec![(doge_block_wrong_bits(), None)];
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(headers)
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;

        assert!(
            format!("{:?}", outcome.failures()[0].clone().into_result())
                .contains("Error: Incorrect target."),
            "Expected 'Error: Incorrect target.' but got: {:?}",
            outcome.failures()
        );
        Ok(())
    }

    /// Submit a Dogecoin block with wrong target bits AND AuxPoW data attached.
    /// Expected: still rejected with "Error: Incorrect target." because the bits
    /// check in check_pow fires before check_aux is ever reached.
    #[tokio::test]
    async fn test_wrong_target_with_auxpow_rejected_before_auxpow_check(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (contract, user_account) = init_dogecoin_contract().await?;

        let aux_data = AuxData {
            coinbase_tx: vec![],
            merkle_proof: vec![],
            chain_merkle_proof: vec![],
            chain_id: 0,
            parent_block: doge_init_blocks()[0].clone(),
        };
        let headers: Vec<(Header, Option<AuxData>)> =
            vec![(doge_block_wrong_bits(), Some(aux_data))];
        let outcome = user_account
            .call(contract.id(), "submit_blocks")
            .args_borsh(headers)
            .deposit(STORAGE_DEPOSIT_PER_BLOCK)
            .max_gas()
            .transact()
            .await?;

        assert!(
            format!("{:?}", outcome.failures()[0].clone().into_result())
                .contains("Error: Incorrect target."),
            "Expected 'Error: Incorrect target.' but got: {:?}",
            outcome.failures()
        );
        Ok(())
    }
}
