use btc_types::contract_args::{InitArgs, ProofArgs};
use btc_types::hash::H256;
use btc_types::header::{ExtendedHeader, Header};
use btc_types::u256::U256;
use near_plugins::{
    access_control, pause, AccessControlRole, AccessControllable, Pausable, Upgradable,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, log, near, require, PanicOnDefault};

// use bitcoin::block::Header;
// mod types;

/// Define roles for access control of `Pausable` features. Accounts which are
/// granted a role are authorized to execute the corresponding action.
#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    /// May pause and unpause features.
    PauseManager,
    /// Allows to use contract API even after contract is paused
    UnrestrictedSubmitBlocks,
    /// Allows to use `verify_transaction` API on a paused contract
    UnrestrictedVerifyTransaction,
    /// May successfully call any of the protected `Upgradable` methods since below it is passed to
    /// every attribute of `access_control_roles`.
    ///
    /// Using this pattern grantees of a single role are authorized to call all `Upgradable`methods.
    DAO,
    /// May successfully call `Upgradable::up_stage_code`, but none of the other protected methods,
    /// since below is passed only to the `code_stagers` attribute.
    ///
    /// Using this pattern grantees of a role are authorized to call only one particular protected
    /// `Upgradable` method.
    CodeStager,
    /// May successfully call `Upgradable::up_deploy_code`, but none of the other protected methods,
    /// since below is passed only to the `code_deployers` attribute.
    ///
    /// Using this pattern grantees of a role are authorized to call only one particular protected
    /// `Upgradable` method.
    CodeDeployer,
    /// May successfully call `Upgradable` methods to initialize and update the staging duration
    /// since below it is passed to the attributes `duration_initializers`,
    /// `duration_update_stagers`, and `duration_update_appliers`.
    ///
    /// Using this pattern grantees of a single role are authorized to call multiple (but not all)
    /// protected `Upgradable` methods.
    DurationManager,
}
#[derive(BorshSerialize, near_sdk::BorshStorageKey)]
enum StorageKey {
    MainchainHeightToHeader,
    MainchainHeaderToHeight,
    HeadersPool,
}

/// Contract implementing Bitcoin light client.
/// See README.md for more details about features and implementation logic behind the code.

/// This contract could work in a pairing with an external off-chain relay service. To learn more about
/// relay, take a look at the relay service documentation.

#[access_control(role_type(Role))]
#[near(contract_state)]
#[derive(Pausable, Upgradable, PanicOnDefault)]
#[pausable(manager_roles(Role::PauseManager))]
#[upgradable(access_control_roles(
    code_stagers(Role::CodeStager, Role::DAO),
    code_deployers(Role::CodeDeployer, Role::DAO),
    duration_initializers(Role::DurationManager, Role::DAO),
    duration_update_stagers(Role::DurationManager, Role::DAO),
    duration_update_appliers(Role::DurationManager, Role::DAO),
))]
pub struct BtcLightClient {
    // A pair of lookup maps that allows to find header by height and height by header
    mainchain_height_to_header: near_sdk::store::LookupMap<u64, H256>,
    mainchain_header_to_height: near_sdk::store::LookupMap<H256, u64>,

    // Block with the highest chainWork, i.e., blockchain tip, you can find latest height inside of it
    mainchain_tip_blockhash: H256,

    // The oldest block in main chain we store
    mainchain_initial_blockhash: H256,

    // Mapping of block hashes to block headers (ALL ever submitted, i.e., incl. forks)
    headers_pool: near_sdk::store::LookupMap<H256, ExtendedHeader>,

    // If we should run all the block checks or not
    skip_pow_verification: bool,

    // GC threshold - how many blocks we would like to store in memory, and GC the older ones
    gc_threshold: u64,
}

#[near]
impl BtcLightClient {
    #[init]
    #[private]
    #[must_use]
    pub fn init(args: InitArgs) -> Self {
        let mut contract = Self {
            mainchain_height_to_header: near_sdk::store::LookupMap::new(
                StorageKey::MainchainHeightToHeader,
            ),
            mainchain_header_to_height: near_sdk::store::LookupMap::new(
                StorageKey::MainchainHeaderToHeight,
            ),
            headers_pool: near_sdk::store::LookupMap::new(StorageKey::HeadersPool),
            mainchain_initial_blockhash: H256::default(),
            mainchain_tip_blockhash: H256::default(),
            skip_pow_verification: args.skip_pow_verification,
            gc_threshold: args.gc_threshold,
        };

        // Make the contract itself super admin. This allows us to grant any role in the
        // constructor.
        near_sdk::require!(
            contract.acl_init_super_admin(env::current_account_id()),
            "Failed to initialize super admin",
        );

        contract.init_genesis(args.genesis_block, args.genesis_block_height);

        contract
    }

    #[pause(except(roles(Role::UnrestrictedSubmitBlocks)))]
    pub fn submit_blocks(&mut self, #[serializer(borsh)] headers: Vec<Header>) {
        for header in headers {
            self.submit_block_header(header);
        }
    }

    pub fn get_last_block_header(&self) -> ExtendedHeader {
        self.headers_pool[&self.mainchain_tip_blockhash].clone()
    }

    pub fn get_block_hash_by_height(&self, height: u64) -> Option<&H256> {
        self.mainchain_height_to_header.get(&height)
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn get_height_by_block_hash(&self, blockhash: H256) -> Option<u64> {
        self.mainchain_header_to_height.get(&blockhash).copied()
    }

    pub fn get_mainchain_size(&self) -> u64 {
        let tail = &self.headers_pool[&self.mainchain_initial_blockhash];
        let tip = &self.headers_pool[&self.mainchain_tip_blockhash];
        tip.block_height - tail.block_height
    }

    /// This method return n last blocks from the mainchain
    /// # Panics
    /// Cannot find a tip of main chain in a pool
    pub fn get_last_n_blocks_hashes(&self, skip: u64, limit: u64) -> Vec<&H256> {
        let mut block_hashes = vec![];
        let tip_hash = &self.mainchain_tip_blockhash;
        let tip = self
            .headers_pool
            .get(tip_hash)
            .unwrap_or_else(|| env::panic_str("heaviest block should be recorded"));

        let min_block_height = self
            .headers_pool
            .get(&self.mainchain_initial_blockhash)
            .unwrap_or_else(|| env::panic_str("initial block should be recorded"))
            .block_height;

        let start_block_height =
            std::cmp::max(min_block_height, tip.block_height - limit - skip + 1);

        for height in start_block_height..=(tip.block_height - skip) {
            if let Some(block_hash) = self.mainchain_height_to_header.get(&height) {
                block_hashes.push(block_hash);
            }
        }

        block_hashes
    }

    /// Verifies that a transaction is included in a block at a given block height
    ///
    /// @param txid transaction identifier
    /// @param txBlockHeight block height at which transacton is supposedly included
    /// @param txIndex index of transaction in the block's tx merkle tree
    /// @param merkleProof  merkle tree path (concatenated LE sha256 hashes) (does not contain initial transaction_hash and merkle_root)
    /// @param confirmations how many confirmed blocks we want to have before the transaction is valid
    /// @return True if txid is at the claimed position in the block at the given blockheight, False otherwise
    ///
    /// # Panics
    /// Multiple cases
    #[allow(clippy::needless_pass_by_value)]
    #[pause(except(roles(Role::UnrestrictedVerifyTransaction)))]
    pub fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: ProofArgs) -> bool {
        let heaviest_block_header = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str("heaviest block must be recorded"));
        let target_block_height = *self
            .mainchain_header_to_height
            .get(&args.tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("block does not belong to the current main chain"));

        // Check requested confirmations. No need to compute proof if insufficient confirmations.
        require!(
            (heaviest_block_header.block_height).saturating_sub(target_block_height)
                >= args.confirmations,
            "Not enough blocks confirmed, cannot process verification"
        );

        let header = self
            .headers_pool
            .get(&args.tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("cannot find requested transaction block"));

        // compute merkle tree root and check if it matches block's original merkle tree root
        merkle_tools::compute_root_from_merkle_proof(
            args.tx_id.clone(),
            usize::try_from(args.tx_index).unwrap(),
            &args.merkle_proof,
        ) == header.block_header.merkle_root
    }

    /// Public call to run GC on a mainchain.
    /// batch_size is how many block headers should be removed in the execution
    ///
    /// # Panics
    /// If initial blockheader or tip blockheader are not in a header pool
    pub fn run_mainchain_gc(&mut self, batch_size: u64) {
        let initial_blockheader = self
            .headers_pool
            .get(&self.mainchain_initial_blockhash)
            .unwrap_or_else(|| env::panic_str("initial blockheader must be in a header pool"));

        let tip_blockheader = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str("tip blockheader must be in a header pool"));

        let amount_of_headers_we_store =
            tip_blockheader.block_height - initial_blockheader.block_height;

        if amount_of_headers_we_store > self.gc_threshold {
            let total_amount_to_remove = amount_of_headers_we_store - self.gc_threshold;
            let selected_amount_to_remove = std::cmp::min(total_amount_to_remove, batch_size);

            let start_removal_height = initial_blockheader.block_height;
            let end_removal_height = initial_blockheader.block_height + selected_amount_to_remove;
            env::log_str(&format!(
                "Num of blocks to remove {selected_amount_to_remove}"
            ));

            for height in start_removal_height..end_removal_height {
                let blockhash = &self.mainchain_height_to_header[&height];

                self.headers_pool.remove(blockhash);
                self.mainchain_header_to_height.remove(blockhash);
                self.mainchain_height_to_header.remove(&height);
            }

            self.mainchain_initial_blockhash
                .clone_from(&self.mainchain_height_to_header[&end_removal_height]);
        }
    }
}

impl BtcLightClient {
    fn init_genesis(&mut self, block_header: Header, block_height: u64) {
        let current_block_hash = block_header.block_hash();
        let chain_work = block_header.work();

        let header = ExtendedHeader {
            block_header,
            block_height,
            block_hash: current_block_hash.clone(),
            chain_work,
        };

        self.store_block_header(header);
        self.mainchain_initial_blockhash
            .clone_from(&current_block_hash);
        self.mainchain_tip_blockhash = current_block_hash;
    }

    fn submit_block_header(&mut self, block_header: Header) {
        // We do not have a previous block in the headers_pool, there is a high probability
        // it means we are starting to receive a new fork,
        // so what we do now is we are returning the error code
        // to ask the relay to deploy the previous block.
        //
        // Offchain relay now, should submit blocks one by one in decreasing height order
        // 80 -> 79 -> 78 -> ...
        // And do it until we can accept the block.
        // It means we found an initial fork position.
        // We are starting to gather new fork from this initial position.
        let prev_block_header = self
            .headers_pool
            .get(&block_header.prev_block_hash)
            .unwrap_or_else(|| env::panic_str("PrevBlockNotFound"));

        let current_block_hash = block_header.block_hash();
        require!(
            self.skip_pow_verification
                || U256::from_le_bytes(&current_block_hash.0) <= block_header.target(),
            "block should have correct pow"
        );

        let (current_block_computed_chain_work, overflow) = prev_block_header
            .chain_work
            .overflowing_add(block_header.work());
        require!(!overflow, "Addition of U256 values overflowed");

        let current_header = ExtendedHeader {
            block_header,
            block_hash: current_block_hash,
            chain_work: current_block_computed_chain_work,
            block_height: 1 + prev_block_header.block_height,
        };

        // Main chain submission
        if prev_block_header.block_hash == self.mainchain_tip_blockhash {
            // Probably we should check if it is not in a mainchain?
            // chainwork > highScore
            log!("Block {}: saving to mainchain", current_header.block_hash);
            // Validate chain
            assert_eq!(
                self.mainchain_tip_blockhash,
                current_header.block_header.prev_block_hash
            );

            self.mainchain_tip_blockhash = current_header.block_hash.clone();
            self.store_block_header(current_header);
        } else {
            log!("Block {}: saving to fork", current_header.block_hash);
            // Fork submission
            let main_chain_tip_header = self
                .headers_pool
                .get(&self.mainchain_tip_blockhash)
                .unwrap_or_else(|| env::panic_str("tip should be in a header pool"));

            let last_main_chain_block_height = main_chain_tip_header.block_height;
            let total_main_chain_chainwork = main_chain_tip_header.chain_work;

            self.store_fork_header(current_header.clone());

            // Current chainwork is higher than on a current mainchain, let's promote the fork
            if current_block_computed_chain_work > total_main_chain_chainwork {
                log!("Chain reorg");
                self.reorg_chain(current_header, last_main_chain_block_height);
            }
        }
    }

    /// The most expensive operation which reorganizes the chain, based on fork weight
    fn reorg_chain(&mut self, fork_tip_header: ExtendedHeader, last_main_chain_block_height: u64) {
        let fork_tip_height = fork_tip_header.block_height;
        if last_main_chain_block_height > fork_tip_height {
            // If we see that main chain is longer than fork we first garbage collect
            // outstanding main chain blocks:
            //
            //      [m1] - [m2] - [m3] - [m4] <- We should remove [m4]
            //     /
            // [m0]
            //     \
            //      [f1] - [f2] - [f3]
            for height in (fork_tip_height + 1)..=last_main_chain_block_height {
                let current_main_chain_blockhash = self
                    .mainchain_height_to_header
                    .get(&height)
                    .unwrap_or_else(|| env::panic_str("cannot get a block"));
                self.mainchain_header_to_height
                    .remove(current_main_chain_blockhash);
                self.headers_pool.remove(current_main_chain_blockhash);
                self.mainchain_height_to_header.remove(&height);
            }
        }

        // Now we are in a situation where mainchain is equivalent to fork size:
        //
        //      [m1] - [m2] - [m3] - [m4] <- main tip
        //     /
        // [m0]
        //     \
        //      [f1] - [f2] - [f3] - [f4] <- fork tip
        //
        //
        // Or in a situation where it is shorter:
        //
        //      [m1] - [m2] - [m3] <- main tip
        //     /
        // [m0]
        //     \
        //      [f1] - [f2] - [f3] - [f4] <- fork tip

        let mut fork_header_cursor = &fork_tip_header;

        while !self
            .mainchain_header_to_height
            .contains_key(&fork_header_cursor.block_hash)
        {
            let prev_block_hash = fork_header_cursor.block_header.prev_block_hash.clone();
            let current_block_hash = fork_header_cursor.block_hash.clone();
            let current_height = fork_header_cursor.block_height;

            // Inserting the fork block into the main chain, if some mainchain block is occupying
            // this height let's save its hashcode
            let main_chain_block = self
                .mainchain_height_to_header
                .insert(current_height, current_block_hash.clone());
            self.mainchain_header_to_height
                .insert(current_block_hash, current_height);

            // If we found a mainchain block at the current height than remove this block from the
            // header pool and from the header -> height map
            if let Some(current_main_chain_blockhash) = main_chain_block {
                self.mainchain_header_to_height
                    .remove(&current_main_chain_blockhash);
                self.headers_pool.remove(&current_main_chain_blockhash);
            }

            // Switch iterator cursor to the previous block in fork
            fork_header_cursor = self
                .headers_pool
                .get_mut(&prev_block_hash)
                .unwrap_or_else(|| env::panic_str("previous fork block should be there"));
        }

        // Updating tip of the new main chain
        self.mainchain_tip_blockhash = fork_tip_header.block_hash;
    }

    /// Stores parsed block header and meta information
    fn store_block_header(&mut self, header: ExtendedHeader) {
        self.mainchain_height_to_header
            .insert(header.block_height, header.block_hash.clone());
        self.mainchain_header_to_height
            .insert(header.block_hash.clone(), header.block_height);
        self.headers_pool.insert(header.block_hash.clone(), header);
    }

    /// Stores and handles fork submissions
    fn store_fork_header(&mut self, header: ExtendedHeader) {
        self.headers_pool.insert(header.block_hash.clone(), header);
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(hex: &str) -> H256 {
        hex.parse().unwrap()
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

    fn get_default_init_args() -> InitArgs {
        InitArgs {
            genesis_block: genesis_block_header(),
            genesis_block_height: 0,
            skip_pow_verification: false,
            gc_threshold: 3,
        }
    }

    fn get_default_init_args_with_skip_pow() -> InitArgs {
        InitArgs {
            genesis_block: genesis_block_header(),
            genesis_block_height: 0,
            skip_pow_verification: true,
            gc_threshold: 3,
        }
    }

    #[test]
    #[should_panic(expected = "block should have correct pow")]
    fn test_pow_validator_works_correctly_for_wrong_block() {
        let header = block_header_example();

        let mut contract = BtcLightClient::init(get_default_init_args());

        contract.submit_block_header(header);
    }

    #[test]
    fn test_pow_validator_works_correctly_for_correct_block() {
        let header = fork_block_header_example();
        let mut contract = BtcLightClient::init(get_default_init_args());

        contract.submit_block_header(header.clone());

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            ExtendedHeader {
                block_header: header,
                block_hash: decode_hex(
                    "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048"
                ),
                chain_work: U256::from_be_bytes(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ]),
                block_height: 1
            }
        );
    }

    #[test]
    fn test_saving_mainchain_block_header() {
        let header = block_header_example();

        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(header.clone());

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            ExtendedHeader {
                block_header: header,
                block_hash: decode_hex(
                    "62703463e75c025987093c6fa96e7261ac982063ea048a0550407ddbbe865345"
                ),
                chain_work: U256::from_be_bytes(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ]),
                block_height: 1
            }
        );
    }

    #[test]
    fn test_submitting_new_fork_block_header() {
        let header = block_header_example();

        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(header.clone());

        contract.submit_block_header(fork_block_header_example());

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            ExtendedHeader {
                block_header: header,
                block_hash: decode_hex(
                    "62703463e75c025987093c6fa96e7261ac982063ea048a0550407ddbbe865345"
                ),
                chain_work: U256::from_be_bytes(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ]),
                block_height: 1
            }
        );
    }

    // test we can insert a block and get block back by it's height
    #[test]
    fn test_getting_block_by_height() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(block_header_example());

        assert_eq!(
            contract.get_block_hash_by_height(0).unwrap(),
            &genesis_block_header().block_hash(),
        );
        assert_eq!(
            contract.get_block_hash_by_height(1).unwrap(),
            &block_header_example().block_hash()
        );
    }

    #[test]
    fn test_getting_height_by_block() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(block_header_example());

        assert_eq!(
            contract
                .get_height_by_block_hash(genesis_block_header().block_hash())
                .unwrap(),
            0
        );
        assert_eq!(
            contract
                .get_height_by_block_hash(block_header_example().block_hash())
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_submitting_existing_fork_block_header_and_promote_fork() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(block_header_example());

        contract.submit_block_header(fork_block_header_example());
        contract.submit_block_header(fork_block_header_example_2());

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            ExtendedHeader {
                block_header: fork_block_header_example_2(),
                block_hash: decode_hex(
                    "000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"
                ),
                chain_work: U256::from_be_bytes(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 3, 0, 3, 0, 3
                ]),
                block_height: 2
            }
        );
    }

    #[test]
    #[should_panic(expected = "PrevBlockNotFound")]
    fn test_getting_an_error_if_submitting_unattached_block() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(fork_block_header_example_2());
    }
}
