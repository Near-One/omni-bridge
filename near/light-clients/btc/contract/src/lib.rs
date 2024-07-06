use near_plugins::{
    access_control, pause, AccessControlRole, AccessControllable, Pausable, Upgradable,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, log, near, PanicOnDefault};

use bitcoin::block::Header;

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

mod state {
    use super::borsh;
    use bitcoin::block::Version;
    use bitcoin::hashes::serde::{Deserialize, Serialize};
    use bitcoin::CompactTarget;
    use near_sdk::borsh::{BorshDeserialize, BorshSerialize};

    /// Bitcoin header to store in the block height
    #[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Clone)]
    #[serde(crate = "near_sdk::serde")]
    pub struct Header {
        /// Block version, now repurposed for soft fork signalling.
        pub version: i32,
        /// Reference to the previous block in the chain.
        pub prev_blockhash: String,
        /// The root hash of the merkle tree of transactions in the block.
        pub merkle_root: String,
        /// The timestamp of the block, as claimed by the miner.
        pub time: u32,
        /// The target value below which the blockhash must lie.
        pub bits: u32,
        /// The nonce, selected to obtain a low enough blockhash.
        pub nonce: u32,

        /// Below, state contains additional fields not presented in the standard blockchain header
        /// those fields are used to represent additional information required for fork management
        /// and other utility functionality
        ///
        /// Current `block_hash`
        pub current_blockhash: String,
        /// Accumulated chainwork at this position for this block (big endian storage format)
        pub chainwork: [u8; 32],
        /// Block height in the Bitcoin network
        pub block_height: u64,
    }

    impl Header {
        pub fn new(header: bitcoin::block::Header, chainwork: [u8; 32], block_height: u64) -> Self {
            Self {
                version: header.version.to_consensus(),
                current_blockhash: header.block_hash().to_string(),
                prev_blockhash: header.prev_blockhash.to_string(),
                merkle_root: header.merkle_root.to_string(),
                time: header.time,
                bits: header.bits.to_consensus(),
                nonce: header.nonce,
                chainwork,
                block_height,
            }
        }

        pub fn to_bitcoin_block_header(&self) -> bitcoin::block::Header {
            let prev_blockhash_json = serde_json::json!(self.prev_blockhash);
            let merkle_root_json = serde_json::json!(self.merkle_root);

            bitcoin::block::Header {
                version: Version::from_consensus(self.version),
                prev_blockhash: serde_json::from_value(prev_blockhash_json).unwrap(),
                merkle_root: serde_json::from_value(merkle_root_json).unwrap(),
                time: self.time,
                bits: CompactTarget::from_consensus(self.bits),
                nonce: self.nonce,
            }
        }
    }
}

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
pub struct Contract {
    // A pair of lookup maps that allows to find header by height and height by header
    mainchain_height_to_header: near_sdk::store::LookupMap<u64, String>,
    mainchain_header_to_height: near_sdk::store::LookupMap<String, u64>,

    // Block with the highest chainWork, i.e., blockchain tip, you can find latest height inside of it
    mainchain_tip_blockhash: String,

    // The oldest block in main chain we store
    mainchain_initial_blockhash: String,

    // Mapping of block hashes to block headers (ALL ever submitted, i.e., incl. forks)
    headers_pool: near_sdk::store::LookupMap<String, state::Header>,

    // If we should run all the block checks or not
    enable_check: bool,

    // GC threshold - how many blocks we would like to store in memory, and GC the older ones
    gc_threshold: u64,
}

// Implement the contract structure
#[near]
impl Contract {
    #[init]
    #[must_use]
    pub fn new(
        genesis_block: Header,
        genesis_block_height: u64,
        enable_check: bool,
        gc_threshold: u64,
    ) -> Self {
        log!("Running the initialization!");

        let mut contract = Self {
            mainchain_height_to_header: near_sdk::store::LookupMap::new(
                StorageKey::MainchainHeightToHeader,
            ),
            mainchain_header_to_height: near_sdk::store::LookupMap::new(
                StorageKey::MainchainHeaderToHeight,
            ),
            headers_pool: near_sdk::store::LookupMap::new(StorageKey::HeadersPool),
            mainchain_initial_blockhash: String::new(),
            mainchain_tip_blockhash: String::new(),
            enable_check,
            gc_threshold,
        };

        // Make the contract itself super admin. This allows us to grant any role in the
        // constructor.
        near_sdk::require!(
            contract.acl_init_super_admin(env::current_account_id()),
            "Failed to initialize super admin",
        );

        contract.init_genesis(genesis_block, genesis_block_height);

        contract
    }

    fn init_genesis(&mut self, block_header: Header, block_height: u64) {
        let current_block_hash = block_header.block_hash().as_raw_hash().to_string();
        let chainwork_bytes = block_header.work().to_be_bytes();

        let header = state::Header::new(block_header, chainwork_bytes, block_height);

        self.store_block_header(current_block_hash.clone(), &header);
        self.mainchain_initial_blockhash
            .clone_from(&current_block_hash);
        self.mainchain_tip_blockhash = current_block_hash;
    }

    pub fn get_last_block_header(&self) -> state::Header {
        self.headers_pool[&self.mainchain_tip_blockhash].clone()
    }

    pub fn get_blockhash_by_height(&self, height: u64) -> Option<String> {
        self.mainchain_height_to_header
            .get(&height)
            .map(std::borrow::ToOwned::to_owned)
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn get_height_by_blockhash(&self, blockhash: String) -> Option<u64> {
        self.mainchain_header_to_height.get(&blockhash).copied()
    }

    /// Saving block header received from a Bitcoin relay service
    /// This method is private but critically important for the overall execution flow
    ///
    /// # Panics
    /// - Many cases
    ///
    /// # Errors
    /// - No previous block recorded, so we cannot validate chain
    #[handle_result]
    #[pause(except(roles(Role::UnrestrictedSubmitBlocks)))]
    pub fn submit_block_header(&mut self, block_header: Header) -> Result<(), String> {
        // Chainwork is validated inside block_header structure (other consistency checks too)
        let prev_blockhash = block_header.prev_blockhash.to_string();

        let prev_block_header = if let Some(header) = self.headers_pool.get(&prev_blockhash) {
            header.clone()
        } else {
            // We do not have a previous block in the headers_pool, there is a high probability
            //it means we are starting to receive a new fork,
            // so what we do now is we are returning the error code
            // to ask the relay to deploy the previous block.
            //
            // Offchain relay now, should submit blocks one by one in decreasing height order
            // 80 -> 79 -> 78 -> ...
            // And do it until we can accept the block.
            // It means we found an initial fork position.
            // We are starting to gather new fork from this initial position.
            return Err(String::from("1"));
        };

        if self.enable_check {
            block_header
                .validate_pow(block_header.target())
                .unwrap_or_else(|_| env::panic_str("block should have correct pow"));
        }

        let current_blockhash = block_header.block_hash().to_string();
        let current_block_computed_chainwork =
            bitcoin::Work::from_be_bytes(prev_block_header.chainwork) + block_header.work();

        // Computing the target height based on the previous block
        let height = 1 + prev_block_header.block_height;
        let header = state::Header::new(
            block_header,
            current_block_computed_chainwork.to_be_bytes(),
            height,
        );

        // Main chain submission
        if prev_block_header.current_blockhash == self.mainchain_tip_blockhash {
            // Probably we should check if it is not in a mainchain?
            // chainwork > highScore
            log!("Saving to mainchain");
            // Validate chain
            assert_eq!(self.mainchain_tip_blockhash, header.prev_blockhash);

            self.store_block_header(current_blockhash.clone(), &header);
            self.mainchain_tip_blockhash = current_blockhash;
        } else {
            // Fork submission
            let main_chain_tip_header = self
                .headers_pool
                .get(&self.mainchain_tip_blockhash.clone())
                .unwrap_or_else(|| env::panic_str("tip should be in a header pool"));

            let total_main_chain_chainwork =
                bitcoin::Work::from_be_bytes(main_chain_tip_header.chainwork);

            self.store_fork_header(current_blockhash.clone(), header.clone());

            // Current chainwork is higher than on a current mainchain, let's promote the fork
            if current_block_computed_chainwork > total_main_chain_chainwork {
                self.reorg_chain(&current_blockhash);
            }
        }

        Ok(())
    }

    /// The most expensive operation which reorganizes the chain, based on fork weight
    fn reorg_chain(&mut self, fork_tip_header_blockhash: &str) {
        let fork_tip_height = self.headers_pool[fork_tip_header_blockhash].block_height;
        let last_main_chain_block_height =
            self.headers_pool[&self.mainchain_tip_blockhash].block_height;

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

        let mut fork_header_cursor = self
            .headers_pool
            .get_mut(fork_tip_header_blockhash)
            .unwrap_or_else(|| env::panic_str("fork block should be already inserted at the time"));

        while !self
            .mainchain_header_to_height
            .contains_key(&fork_header_cursor.current_blockhash)
        {
            let prev_blockhash = fork_header_cursor.prev_blockhash.clone();
            let current_blockhash = fork_header_cursor.current_blockhash.clone();
            let current_height = fork_header_cursor.block_height;

            // Inserting the fork block into the main chain, if some mainchain block is occupying
            // this height let's save its hashcode
            let main_chain_block = self
                .mainchain_height_to_header
                .insert(current_height, current_blockhash.clone());
            self.mainchain_header_to_height
                .insert(current_blockhash, current_height);

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
                .get_mut(&prev_blockhash)
                .unwrap_or_else(|| env::panic_str("previous fork block should be there"));
        }

        // Updating tip of the new main chain
        self.mainchain_tip_blockhash = fork_tip_header_blockhash.to_string();
    }

    /// Stores parsed block header and meta information
    fn store_block_header(&mut self, current_block_hash: String, header: &state::Header) {
        self.headers_pool
            .insert(current_block_hash.clone(), header.clone());
        self.mainchain_height_to_header
            .insert(header.block_height, current_block_hash.clone());
        self.mainchain_header_to_height
            .insert(current_block_hash, header.block_height);
    }

    /// Stores and handles fork submissions
    fn store_fork_header(&mut self, current_block_hash: String, header: state::Header) {
        self.headers_pool.insert(current_block_hash, header);
    }

    /// This method return n last blocks from the mainchain
    /// # Panics
    /// Cannot find a tip of main chain in a pool
    pub fn receive_last_n_blocks(&self, n: u64, shift_from_the_end: u64) -> Vec<String> {
        let mut block_hashes = vec![];
        let tip_hash = &self.mainchain_tip_blockhash;
        let tip = self
            .headers_pool
            .get(tip_hash)
            .unwrap_or_else(|| env::panic_str("heaviest block should be recorded"));

        for height in (tip.block_height - n)..(tip.block_height - shift_from_the_end) {
            if let Some(block_hash) = self.mainchain_height_to_header.get(&height) {
                block_hashes.push(block_hash.to_string());
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
    pub fn verify_transaction_inclusion(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        confirmations: u64,
    ) -> bool {
        let heaviest_block_header = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str("heaviest block must be recorded"));
        let target_block_height = *self
            .mainchain_header_to_height
            .get(&tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("block does not belong to the current main chain"));

        // Check requested confirmations. No need to compute proof if insufficient confirmations.
        assert!(
            (heaviest_block_header.block_height).saturating_sub(target_block_height)
                >= confirmations,
            "Not enough blocks confirmed, cannot process verification"
        );

        let header = self
            .headers_pool
            .get(&tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("cannot find requested transaction block"));
        let merkle_root = header.clone().merkle_root;

        // compute merkle tree root and check if it matches block's original merkle tree root
        if merkle_tools::compute_root_from_merkle_proof(
            &tx_id,
            usize::try_from(tx_index).unwrap(),
            &merkle_proof,
        ) == merkle_root
        {
            log!(
                "VerityTransaction: Tx {:?} is included in block with height {}",
                tx_id,
                target_block_height
            );
            true
        } else {
            log!(
                "VerityTransaction: Tx {:?} is NOT included in block with height {}",
                tx_id,
                target_block_height
            );
            false
        }
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

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::block::Header;

    fn genesis_block_header() -> Header {
        let json_value = serde_json::json!({
            "version": 1,
            "prev_blockhash": "0000000000000000000000000000000000000000000000000000000000000000",
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
            "prev_blockhash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
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
            "prev_blockhash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
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
          "prev_blockhash": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048",
        });

        serde_json::from_value(json_value).expect("value is invalid")
    }

    #[test]
    #[should_panic(expected = "block should have correct pow")]
    fn test_pow_validator_works_correctly_for_wrong_block() {
        let header = block_header_example();

        let mut contract = Contract::new(genesis_block_header(), 0, true, 3);

        let _ = contract.submit_block_header(header);
    }

    #[test]
    fn test_pow_validator_works_correctly_for_correct_block() {
        let header = fork_block_header_example();
        let mut contract = Contract::new(genesis_block_header(), 0, true, 3);

        contract.submit_block_header(header).unwrap();

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            state::Header::new(
                header,
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ],
                1
            )
        );
    }

    #[test]
    fn test_saving_mainchain_block_header() {
        let header = block_header_example();

        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        contract.submit_block_header(header).unwrap();

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            state::Header::new(
                header,
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ],
                1
            )
        );
    }

    #[test]
    fn test_submitting_new_fork_block_header() {
        let header = block_header_example();

        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        contract.submit_block_header(header).unwrap();

        contract
            .submit_block_header(fork_block_header_example())
            .unwrap();

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            state::Header::new(
                header,
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 2, 0, 2, 0, 2
                ],
                1
            )
        );
    }

    // test we can insert a block and get block back by it's height
    #[test]
    fn test_getting_block_by_height() {
        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        contract
            .submit_block_header(block_header_example())
            .unwrap();

        assert_eq!(
            contract.get_blockhash_by_height(0).unwrap(),
            genesis_block_header().block_hash().to_string()
        );
        assert_eq!(
            contract.get_blockhash_by_height(1).unwrap(),
            block_header_example().block_hash().to_string()
        );
    }

    #[test]
    fn test_getting_height_by_block() {
        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        contract
            .submit_block_header(block_header_example())
            .unwrap();

        assert_eq!(
            contract
                .get_height_by_blockhash(genesis_block_header().block_hash().to_string())
                .unwrap(),
            0
        );
        assert_eq!(
            contract
                .get_height_by_blockhash(block_header_example().block_hash().to_string())
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_submitting_existing_fork_block_header_and_promote_fork() {
        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        contract
            .submit_block_header(block_header_example())
            .unwrap();

        contract
            .submit_block_header(fork_block_header_example())
            .unwrap();
        contract
            .submit_block_header(fork_block_header_example_2())
            .unwrap();

        let received_header = contract.get_last_block_header();

        assert_eq!(
            received_header,
            state::Header::new(
                fork_block_header_example_2(),
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 3, 0, 3, 0, 3
                ],
                2
            )
        );
    }

    #[test]
    fn test_getting_an_error_if_submitting_unattached_block() {
        let mut contract = Contract::new(genesis_block_header(), 0, false, 3);

        let result = contract.submit_block_header(fork_block_header_example_2());
        assert!(result.is_err());
        assert!(result.is_err_and(|value| value == *"1"));
    }
}
