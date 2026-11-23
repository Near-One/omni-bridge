use btc_types::contract_args::{
    InitArgs, ProofArgs, ProofArgsV2, TxBlockMeta, TxInclusionInfo, TxInclusionProof,
};
use btc_types::hash::H256;
use btc_types::header::{BlockHeader, ExtendedHeader, ForkTip, Header, LightHeader};
use btc_types::network::Network;
use btc_types::u256::U256;
#[cfg(not(feature = "dogecoin"))]
use btc_types::utils::target_from_bits;
use btc_types::utils::work_from_bits;
use near_plugins::{
    access_control, access_control_any, pause, AccessControlRole, AccessControllable, Pausable,
    Upgradable,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, log, near, require, NearToken, PanicOnDefault, Promise, PromiseOrValue};
use omni_utils::macros::trusted_relayer;

use crate::utils::BlocksGetter;

pub(crate) const ERR_KEY_NOT_EXIST: &str = "ERR_KEY_NOT_EXIST";

/// How far below the main chain tip a fork may branch off to be still worth tracking
pub(crate) const DEFAULT_MAX_REORG: u64 = 100;

mod utils;

#[cfg(feature = "zcash")]
mod zcash;

#[cfg(feature = "dogecoin")]
mod dogecoin;

#[cfg(feature = "bitcoin")]
mod bitcoin;

#[cfg(feature = "litecoin")]
mod litecoin;

/// Define roles for access control of `Pausable` features. Accounts which are
/// granted a role are authorized to execute the corresponding action.
#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    /// May pause and unpause features.
    PauseManager,
    /// Allows to use contract API even after contract is paused
    UnrestrictedSubmitBlocks,
    // Allows to use the GC API on a paused contract
    UnrestrictedRunGC,
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
    /// May manage trusted relayer staking: reject applications and update relayer config.
    RelayerManager,
    UnpauseManager,
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
#[pausable(
    pause_roles(Role::PauseManager),
    unpause_roles(Role::DAO, Role::UnpauseManager)
)]
#[upgradable(access_control_roles(
    code_stagers(Role::CodeStager, Role::DAO),
    code_deployers(Role::CodeDeployer, Role::DAO),
    duration_initializers(Role::DurationManager, Role::DAO),
    duration_update_stagers(Role::DurationManager, Role::DAO),
    duration_update_appliers(Role::DurationManager, Role::DAO),
))]
pub struct BtcLightClient {
    // A pair of lookup maps that allows to find header by height and height by header
    mainchain_height_to_header: LookupMap<u64, H256>,
    mainchain_header_to_height: LookupMap<H256, u64>,

    // Block with the highest chainWork, i.e., blockchain tip, you can find latest height inside of it
    mainchain_tip_blockhash: H256,

    // The oldest block in main chain we store
    mainchain_initial_blockhash: H256,

    // Mapping of block hashes to block headers (ALL ever submitted, i.e., incl. forks)
    headers_pool: LookupMap<H256, ExtendedHeader>,

    // If we should run all the block checks or not
    skip_pow_verification: bool,

    // GC threshold - how many blocks we would like to store in memory, and GC the older ones
    gc_threshold: u64,

    // Forks branching off deeper than this below the main chain tip are collected by the GC
    max_reorg: u64,

    // Network type Mainnet/Testnet
    network: Network,

    // Blocks below this height skip PoW checks while the initial history is loaded; 0 once done
    bootstrap_end_height: u64,

    // Tips of all the tracked forks, sorted by `lca_height` in ascending order: fresh forks
    // branch off close to the main chain tip, so the end of the vector is the part modified
    // often, while the beginning is consumed by GC
    forks_tips: Vec<ForkTip>,
}

#[trusted_relayer(
    bypass_roles(Role::DAO, Role::UnrestrictedSubmitBlocks),
    manager_roles(Role::DAO, Role::RelayerManager),
    config_roles(Role::DAO)
)]
#[near]
impl BtcLightClient {
    /// Recommended initialization parameters:
    /// * `genesis_block_height % difficulty_adjustment_interval == 0`: The genesis block height must be divisible by `difficulty_adjustment_interval` to align with difficulty adjustment cycles.
    /// * The `genesis_block` must be at least 144 blocks earlier than the last block. 144 is the approximate number of blocks generated in one day.
    /// * `skip_pow_verification = false`: Should be set to `false` for standard use. Set to `true` only for testing purposes.
    /// * `gc_threshold = 52704`: This is the approximate number of blocks generated in a year.
    #[init]
    #[private]
    #[must_use]
    pub fn init(args: InitArgs) -> Self {
        let mut contract = Self {
            mainchain_height_to_header: LookupMap::new(StorageKey::MainchainHeightToHeader),
            mainchain_header_to_height: LookupMap::new(StorageKey::MainchainHeaderToHeight),
            headers_pool: LookupMap::new(StorageKey::HeadersPool),
            mainchain_initial_blockhash: H256::default(),
            mainchain_tip_blockhash: H256::default(),
            skip_pow_verification: args.skip_pow_verification,
            gc_threshold: args.gc_threshold,
            max_reorg: std::cmp::min(DEFAULT_MAX_REORG, args.gc_threshold),
            network: args.network,
            bootstrap_end_height: 0,
            forks_tips: Vec::new(),
        };

        // Make the contract itself super admin. This allows us to grant any role in the
        // constructor.
        near_sdk::require!(
            contract.acl_init_super_admin(env::current_account_id()),
            "Failed to initialize super admin",
        );

        contract.init_genesis(
            &args.genesis_block_hash,
            args.genesis_block_height,
            args.genesis_block,
        );

        contract
    }

    /// This method submits provided headers
    /// # Panics
    /// Cannot parse headers len as u64
    #[payable]
    #[pause]
    #[trusted_relayer]
    pub fn submit_blocks(
        &mut self,
        #[serializer(borsh)] headers: Vec<BlockHeader>,
    ) -> PromiseOrValue<()> {
        require!(self.bootstrap_end_height == 0, "ERR_BOOTSTRAP_IN_PROGRESS");

        let amount = env::attached_deposit();
        let initial_storage = env::storage_usage();
        let num_of_headers = headers.len().try_into().unwrap();

        for header in headers {
            self.submit_block_header(header, self.skip_pow_verification);
        }

        self.run_mainchain_gc(num_of_headers);
        self.run_forks_gc(num_of_headers);
        Self::charge_storage_deposit(amount, initial_storage)
    }

    /// Submits the initial history after `init` without proof of work checks and GC
    #[payable]
    #[private]
    #[pause]
    pub fn bootstrap_blocks(
        &mut self,
        #[serializer(borsh)] headers: Vec<BlockHeader>,
    ) -> PromiseOrValue<()> {
        require!(self.bootstrap_end_height > 0, "ERR_BOOTSTRAP_FINISHED");

        let amount = env::attached_deposit();
        let initial_storage = env::storage_usage();

        for header in headers {
            #[cfg(feature = "dogecoin")]
            let header = header.0;
            self.submit_bootstrap_block_header(header);
        }

        self.finish_bootstrap_if_done();
        Self::charge_storage_deposit(amount, initial_storage)
    }

    /// Height up to which blocks are accepted via `bootstrap_blocks`; 0 once bootstrap is finished
    pub fn get_bootstrap_end_height(&self) -> u64 {
        self.bootstrap_end_height
    }

    pub fn get_last_block_header(&self) -> ExtendedHeader {
        self.headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST))
    }

    pub fn get_last_block_height(&self) -> u64 {
        self.headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST))
            .block_height
    }

    pub fn get_block_hash_by_height(&self, height: u64) -> Option<H256> {
        self.mainchain_height_to_header.get(&height)
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn get_height_by_block_hash(&self, blockhash: H256) -> Option<u64> {
        self.mainchain_header_to_height.get(&blockhash)
    }

    pub fn get_mainchain_size(&self) -> u64 {
        let tail = self
            .headers_pool
            .get(&self.mainchain_initial_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));
        let tip = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));
        tip.block_height - tail.block_height + 1
    }

    /// This method return n last blocks from the mainchain
    /// # Panics
    /// Cannot find a tip of main chain in a pool
    pub fn get_last_n_blocks_hashes(&self, skip: u64, limit: u64) -> Vec<H256> {
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

    pub fn get_max_reorg(&self) -> u64 {
        self.max_reorg
    }

    /// Returns the tracked fork tips, sorted by the height of their lowest common ancestor
    /// with the main chain in ascending order, so the most recent forks come last
    ///
    /// @param `skip` how many tips to skip from the beginning of the list
    /// @param `limit` how many tips to return at most
    pub fn get_forks_tips(&self, skip: u64, limit: u64) -> Vec<ForkTip> {
        self.forks_tips
            .iter()
            .skip(usize::try_from(skip).unwrap_or(usize::MAX))
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    /// Verifies that a transaction is included in a block at a given block height
    ///
    /// # Deprecated
    /// Use [`verify_transaction_inclusion_v2`] instead, which includes coinbase merkle proof validation
    /// to mitigate the 64-byte transaction Merkle proof forgery vulnerability:
    /// <https://www.bitmex.com/blog/64-Byte-Transactions>
    ///
    /// @param `tx_id` transaction identifier
    /// @param `tx_block_blockhash` block hash at which transacton is supposedly included
    /// @param `tx_index` index of transaction in the block's tx merkle tree
    /// @param `merkle_proof` merkle tree path (concatenated LE sha256 hashes) (does not contain initial `transaction_hash` and `merkle_root`)
    /// @param confirmations how many confirmed blocks we want to have before the transaction is valid
    /// @return True if `tx_id` is at the claimed position in the block at the given blockhash, False otherwise
    ///
    /// # Warning
    /// This function may return `true` if the provided `tx_id` is a hash of an internal node in the Merkle tree rather than a valid transaction hash.
    /// We assume that validation of whether the `tx_id` corresponds to a valid transaction hash is performed at a higher level of verification.
    ///
    /// # Panics
    /// Multiple cases
    #[deprecated(
        since = "0.5.0",
        note = "Use `verify_transaction_inclusion_v2` instead."
    )]
    #[pause]
    pub fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: ProofArgs) -> bool {
        require!(
            args.confirmations <= self.gc_threshold,
            "The required number of confirmations exceeds the number of blocks stored in memory"
        );

        let heaviest_block_header = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));
        let target_block_height = self
            .mainchain_header_to_height
            .get(&args.tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("block does not belong to the current main chain"));

        // Check requested confirmations. No need to compute proof if insufficient confirmations.
        require!(
            (heaviest_block_header.block_height).saturating_sub(target_block_height) + 1
                >= args.confirmations,
            "Not enough blocks confirmed"
        );

        let header = self
            .headers_pool
            .get(&args.tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("cannot find requested transaction block"));

        require!(!args.merkle_proof.is_empty(), "Merkle proof is empty");

        // compute merkle tree root and check if it matches block's original merkle tree root
        merkle_tools::compute_root_from_merkle_proof(
            args.tx_id,
            usize::try_from(args.tx_index).unwrap(),
            &args.merkle_proof,
        ) == Some(header.block_header.merkle_root)
    }

    /// Same SPV + coinbase checks as `verify_transaction_inclusion_v2`, but returns
    /// block heights instead of a bool and does not enforce a `confirmations`
    /// threshold (the caller can derive the confirmation depth from the returned heights).
    ///
    /// @param `args` see `TxInclusionProof`
    /// @return `Some(TxInclusionInfo)` with the block heights and the most dangerous fork if the
    ///         referenced block is part of the current main chain and the merkle proof
    ///         reconstructs the block's merkle root; `None` if the merkle proof does not match.
    ///
    /// # Warning
    /// This function does not protect against `tx_id` being the hash of an internal
    /// Merkle node rather than a real transaction (see `verify_transaction_inclusion_v2`):
    /// callers MUST validate independently that `tx_id` corresponds to a real transaction.
    ///
    /// # Panics
    /// - if `merkle_proof` and `coinbase_merkle_proof` have different lengths;
    /// - if the coinbase merkle proof does not reconstruct the block's merkle root;
    /// - if the referenced block is not part of the current main chain;
    /// - if the referenced block header is missing from storage;
    /// - if `merkle_proof` is empty.
    #[pause]
    pub fn verify_transaction_inclusion_with_heights(
        &self,
        #[serializer(borsh)] args: TxInclusionProof,
    ) -> Option<TxInclusionInfo> {
        require!(
            args.merkle_proof.len() == args.coinbase_merkle_proof.len(),
            "Coinbase merkle proof and transaction merkle proof should have the same length"
        );

        let meta = self.lookup_tx_block_meta(&args.tx_block_blockhash);

        require!(
            merkle_tools::compute_root_from_merkle_proof(
                args.coinbase_tx_id,
                0usize,
                &args.coinbase_merkle_proof,
            ) == Some(meta.expected_merkle_root.clone()),
            "Incorrect coinbase merkle proof"
        );

        require!(!args.merkle_proof.is_empty(), "Merkle proof is empty");

        let computed_root = merkle_tools::compute_root_from_merkle_proof(
            args.tx_id,
            usize::try_from(args.tx_index).unwrap(),
            &args.merkle_proof,
        );

        (computed_root == Some(meta.expected_merkle_root)).then_some(TxInclusionInfo {
            tx_block_height: meta.target_block_height,
            mainchain_tip_height: meta.tip_block_height,
            dangerous_fork_tip_height: self.dangerous_fork_tip_height(meta.target_block_height),
        })
    }

    /// Verifies that a transaction is included in a block at a given block height,
    /// with an additional coinbase merkle proof validation.
    /// This is needed to mitigate the 64-byte transaction Merkle proof forgery vulnerability:
    /// <https://www.bitmex.com/blog/64-Byte-Transactions>
    ///
    /// @param `tx_id` transaction identifier
    /// @param `tx_block_blockhash` block hash at which transaction is supposedly included
    /// @param `tx_index` index of transaction in the block's tx merkle tree
    /// @param `merkle_proof` merkle tree path (concatenated LE sha256 hashes) (does not contain initial `transaction_hash` and `merkle_root`)
    /// @param `coinbase_tx_id` coinbase transaction hash
    /// @param `coinbase_merkle_proof` merkle proof for the coinbase transaction (must have the same length as `merkle_proof`)
    /// @param confirmations how many confirmed blocks we want to have before the transaction is valid
    /// @return True if `tx_id` is at the claimed position in the block at the given blockhash, False otherwise
    ///
    /// # Security: the 64-byte transaction forgery
    /// A leaf txid is `SHA256d(raw tx bytes)`, while an interior node is `SHA256d(left || right)` —
    /// two 32-byte child hashes concatenated (64 bytes). A *real* 64-byte transaction is therefore
    /// hashed exactly like an interior node. An attacker crafts and includes a genuine 64-byte tx
    /// `T = A || B`, then pretends `txid(T)` is an interior node with "children" `A` (left) and `B`
    /// (right). This lets them prove that `B` (which is not a real transaction) is included, using
    /// `A` as its left sibling plus `T`'s real path to the root. The forgeable half is the right one
    /// `B`: the left half `A` is pinned by the fixed header fields at the start of a transaction
    /// (version, input count, prevout), whereas the tail (last output + locktime) can be stuffed
    /// with arbitrary bytes. The forged leaf `B` sits one level below the real leaves, so its proof
    /// is one longer than any genuine transaction's. v2 blocks this by also
    /// requiring a coinbase proof (leaf index 0) of equal length: since all genuine leaves sit at
    /// the same depth, this pins `tx_id` to a real leaf position and rejects the deeper forged `A`.
    /// See <https://www.bitmex.com/blog/64-Byte-Transactions>
    ///
    /// # Warning
    /// This function does not protect against `tx_id` being the hash of an internal Merkle node
    /// rather than a real transaction. It receives only the `tx_id` hash, not the transaction
    /// bytes, so it cannot tell whether that hash belongs to a correct, well-formed transaction.
    /// Validating this is the responsibility of the caller.
    ///
    /// # Panics
    /// - If `merkle_proof` and `coinbase_merkle_proof` have different lengths
    /// - If `tx_block_blockhash` is not found in the headers pool
    /// - If coinbase merkle proof does not match the block's merkle root
    /// - If the required number of confirmations exceeds the number of stored blocks
    /// - If the block does not belong to the current main chain
    /// - If there are not enough confirmed blocks
    #[pause]
    pub fn verify_transaction_inclusion_v2(&self, #[serializer(borsh)] args: ProofArgsV2) -> bool {
        require!(
            args.confirmations <= self.gc_threshold,
            "The required number of confirmations exceeds the number of blocks stored in memory"
        );

        let confirmations = args.confirmations;

        match self.verify_transaction_inclusion_with_heights(args.into()) {
            Some(TxInclusionInfo {
                tx_block_height,
                mainchain_tip_height,
                ..
            }) => {
                require!(
                    mainchain_tip_height.saturating_sub(tx_block_height) + 1 >= confirmations,
                    "Not enough blocks confirmed"
                );
                true
            }
            None => false,
        }
    }

    /// Sets how far below the main chain tip a fork may branch off to be tracked
    ///
    /// # Panics
    /// If `max_reorg` exceeds `gc_threshold`: the LCA of a fork has to stay in storage
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_reorg(&mut self, max_reorg: u64) {
        require!(
            max_reorg <= self.gc_threshold,
            "ERR_MAX_REORG_ABOVE_GC_THRESHOLD"
        );

        log!("Max reorg set to {}", max_reorg);
        self.max_reorg = max_reorg;
    }

    /// Public call to run GC on a mainchain.
    /// `batch_size` is how many block headers should be removed in the execution
    ///
    /// # Panics
    /// If initial blockheader or tip blockheader are not in a header pool
    #[pause(except(roles(Role::UnrestrictedRunGC)))]
    pub fn run_mainchain_gc(&mut self, batch_size: u64) {
        let initial_blockheader = self
            .headers_pool
            .get(&self.mainchain_initial_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));

        let tip_blockheader = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));

        let amount_of_headers_we_store =
            tip_blockheader.block_height - initial_blockheader.block_height + 1;

        if amount_of_headers_we_store > self.gc_threshold {
            let total_amount_to_remove = amount_of_headers_we_store - self.gc_threshold;
            let selected_amount_to_remove = std::cmp::min(total_amount_to_remove, batch_size);

            let start_removal_height = initial_blockheader.block_height;
            let end_removal_height = initial_blockheader.block_height + selected_amount_to_remove;
            env::log_str(&format!(
                "Num of blocks to remove {selected_amount_to_remove}"
            ));

            for height in start_removal_height..end_removal_height {
                let blockhash = &self
                    .mainchain_height_to_header
                    .get(&height)
                    .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));

                self.remove_block_header(blockhash);
                self.mainchain_height_to_header.remove(&height);
            }

            self.mainchain_initial_blockhash = self
                .mainchain_height_to_header
                .get(&end_removal_height)
                .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));
        }
    }

    /// Public call to run GC on forks. Removes the forks branching off deeper than
    /// `max_reorg` below the main chain tip.
    /// `batch_size` is how many block headers should be removed in the execution
    ///
    /// # Panics
    /// If tip blockheader is not in a header pool
    #[pause(except(roles(Role::UnrestrictedRunGC)))]
    pub fn run_forks_gc(&mut self, batch_size: u64) {
        let cutoff_height = self.get_last_block_height().saturating_sub(self.max_reorg);
        // The outdated forks are a prefix of `forks_tips`, as it is sorted by `lca_height`
        let outdated_forks = self
            .forks_tips
            .partition_point(|fork_tip| fork_tip.lca_height < cutoff_height);

        if outdated_forks == 0 {
            return;
        }

        let mut budget = batch_size;
        let mut removed_forks = 0;

        for index in 0..outdated_forks {
            let fork_tip = self.forks_tips[index].clone();
            let (removed, block_left) = self.remove_fork_blocks(&fork_tip, budget);
            budget -= removed;

            if let Some((tip_hash, tip_height)) = block_left {
                // Leave the tip on the highest block left, so that the next call resumes here
                self.forks_tips[index].tip_hash = tip_hash;
                self.forks_tips[index].tip_height = tip_height;
                break;
            }

            removed_forks = index + 1;

            if budget == 0 {
                break;
            }
        }

        env::log_str(&format!("Num of forks removed {removed_forks}"));

        self.forks_tips.drain(..removed_forks);
        self.recompute_prefix_max_tip_height(0);
    }
}

impl BtcLightClient {
    fn lookup_tx_block_meta(&self, tx_block_blockhash: &H256) -> TxBlockMeta {
        let heaviest_block_header = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST));
        let target_block_height = self
            .mainchain_header_to_height
            .get(tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("block does not belong to the current main chain"));
        let header = self
            .headers_pool
            .get(tx_block_blockhash)
            .unwrap_or_else(|| env::panic_str("cannot find requested transaction block"));
        TxBlockMeta {
            target_block_height,
            tip_block_height: heaviest_block_header.block_height,
            expected_merkle_root: header.block_header.merkle_root,
        }
    }

    fn init_genesis(&mut self, block_hash: &H256, block_height: u64, block_header: Header) {
        env::log_str(&format!(
            "Init with block hash {block_hash} at height {block_height}"
        ));

        #[cfg(feature = "bitcoin")]
        {
            let config = self.get_config();
            require!(block_height.is_multiple_of(config.difficulty_adjustment_interval), format!("Error: The initial block height must be divisible by {} to ensure proper alignment with difficulty adjustment periods.", config.difficulty_adjustment_interval));
        }
        #[cfg(any(feature = "litecoin", feature = "dogecoin"))]
        {
            let config = self.get_config();
            require!((block_height + 1).is_multiple_of(config.difficulty_adjustment_interval), format!("Error: The initial block height  + 1 must be divisible by {} to ensure proper alignment with difficulty adjustment periods.", config.difficulty_adjustment_interval));
        }

        let current_block_hash = block_header.block_hash();
        require!(&current_block_hash == block_hash, "Invalid block hash");
        let chain_work = work_from_bits(block_header.bits);

        let header = ExtendedHeader {
            block_header: block_header.into_light(),
            block_height,
            block_hash: current_block_hash.clone(),
            chain_work,
        };

        self.store_block_header(&header);
        self.mainchain_initial_blockhash
            .clone_from(&current_block_hash);
        self.mainchain_tip_blockhash = current_block_hash;
        if !self.skip_pow_verification {
            self.bootstrap_end_height = block_height + self.bootstrap_blocks_count(block_height);
        }
    }

    #[cfg(not(feature = "zcash"))]
    #[allow(clippy::unused_self)]
    fn bootstrap_blocks_count(&self, _genesis_block_height: u64) -> u64 {
        u64::try_from(btc_types::network::MEDIAN_TIME_SPAN).unwrap() + 1
    }

    fn finish_bootstrap_if_done(&mut self) {
        if self.bootstrap_end_height == 0 {
            return;
        }
        let tip_height = self
            .headers_pool
            .get(&self.mainchain_tip_blockhash)
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST))
            .block_height;
        if tip_height + 1 >= self.bootstrap_end_height {
            log!("Bootstrap finished at height {}", tip_height);
            self.bootstrap_end_height = 0;
        }
    }

    fn submit_bootstrap_block_header(&mut self, header: Header) {
        let prev_block_header = self.get_prev_header(&header.clone().into_light());
        require!(
            prev_block_header.block_hash == self.mainchain_tip_blockhash,
            "ERR_BOOTSTRAP_BLOCK_MUST_EXTEND_TIP"
        );
        let block_height = prev_block_header.block_height + 1;
        require!(
            block_height < self.bootstrap_end_height,
            "ERR_BLOCK_ABOVE_BOOTSTRAP_END"
        );

        let (chain_work, overflow) = prev_block_header
            .chain_work
            .overflowing_add(work_from_bits(header.bits));
        require!(!overflow, "Addition of U256 values overflowed");

        let current_header = ExtendedHeader {
            block_hash: header.block_hash(),
            block_header: header.into_light(),
            chain_work,
            block_height,
        };
        self.submit_block_header_inner(current_header, &prev_block_header);
    }

    fn charge_storage_deposit(amount: NearToken, initial_storage: u64) -> PromiseOrValue<()> {
        let diff_storage_usage = env::storage_usage().saturating_sub(initial_storage);
        let required_deposit = env::storage_byte_cost().saturating_mul(diff_storage_usage.into());

        require!(
            amount >= required_deposit,
            format!("Required deposit {}", required_deposit)
        );

        let refund = amount.saturating_sub(required_deposit);
        if refund > NearToken::from_near(0) {
            Promise::new(env::predecessor_account_id())
                .transfer(refund)
                .into()
        } else {
            PromiseOrValue::Value(())
        }
    }

    #[cfg(not(feature = "dogecoin"))]
    #[allow(clippy::needless_pass_by_value)]
    fn submit_block_header(&mut self, header: Header, skip_pow_verification: bool) {
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
        #[allow(clippy::useless_conversion)]
        let prev_block_header = self.get_prev_header(&header.clone().into());
        let current_block_hash = header.block_hash();

        let (current_block_computed_chain_work, overflow) = prev_block_header
            .chain_work
            .overflowing_add(work_from_bits(header.bits));
        require!(!overflow, "Addition of U256 values overflowed");

        let current_header = ExtendedHeader {
            block_header: header.clone().into_light(),
            block_hash: current_block_hash,
            chain_work: current_block_computed_chain_work,
            block_height: 1 + prev_block_header.block_height,
        };

        if !skip_pow_verification {
            self.check_target(&header, &prev_block_header);

            let pow_hash = header.block_hash_pow();
            // Check if the block hash is less than or equal to the target
            require!(
                U256::from_le_bytes(&pow_hash.0) <= target_from_bits(header.bits),
                format!("block should have correct pow")
            );
        }

        self.submit_block_header_inner(current_header, &prev_block_header);
    }

    fn submit_block_header_inner(
        &mut self,
        current_header: ExtendedHeader,
        prev_block_header: &ExtendedHeader,
    ) {
        // Main chain submission
        if prev_block_header.block_hash == self.mainchain_tip_blockhash {
            // Probably we should check if it is not in a mainchain?
            // chainwork > highScore
            log!(
                "Block {} at height {}: saving to mainchain",
                current_header.block_hash,
                current_header.block_height
            );
            // Validate chain
            assert_eq!(
                self.mainchain_tip_blockhash,
                current_header.block_header.prev_block_hash
            );

            self.store_block_header(&current_header);
            self.mainchain_tip_blockhash = current_header.block_hash;
        } else {
            log!(
                "Block {} at height {}: saving to fork",
                current_header.block_hash,
                current_header.block_height
            );
            // Fork submission
            let main_chain_tip_header = self
                .headers_pool
                .get(&self.mainchain_tip_blockhash)
                .unwrap_or_else(|| env::panic_str("tip should be in a header pool"));

            let last_main_chain_block_height = main_chain_tip_header.block_height;
            let total_main_chain_chainwork = main_chain_tip_header.chain_work;

            self.update_forks_tips(&current_header, last_main_chain_block_height);
            self.store_fork_header(&current_header);

            // Current chainwork is higher than on a current mainchain, let's promote the fork
            if current_header.chain_work > total_main_chain_chainwork {
                log!(
                    "Chain reorg: new tip {} at height {}, replacing tip {} at height {}",
                    current_header.block_hash,
                    current_header.block_height,
                    main_chain_tip_header.block_hash,
                    last_main_chain_block_height
                );
                self.reorg_chain(current_header, last_main_chain_block_height);
            }
        }
    }

    fn check_target(&self, block_header: &Header, prev_block_header: &ExtendedHeader) {
        self.check_pow(block_header, prev_block_header);
    }

    /// The most expensive operation which reorganizes the chain, based on fork weight
    fn reorg_chain(&mut self, fork_tip_header: ExtendedHeader, last_main_chain_block_height: u64) {
        let fork_tip_height = fork_tip_header.block_height;
        let old_main_chain_tip_hash = self.mainchain_tip_blockhash.clone();

        if last_main_chain_block_height > fork_tip_height {
            // If we see that main chain is longer than fork we first unlink the outstanding
            // main chain blocks, keeping them in the pool:
            //
            //      [m1] - [m2] - [m3] - [m4] <- [m4] is not in the main chain anymore
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
                    .remove(&current_main_chain_blockhash);
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

        let fork_tip_hash = fork_tip_header.block_hash.clone();
        let mut fork_header_cursor = fork_tip_header;

        while !self
            .mainchain_header_to_height
            .contains_key(&fork_header_cursor.block_hash)
        {
            let prev_block_hash = fork_header_cursor.block_header.prev_block_hash;
            let current_block_hash = fork_header_cursor.block_hash;
            let current_height = fork_header_cursor.block_height;

            // Inserting the fork block into the main chain, if some mainchain block is occupying
            // this height let's save its hashcode
            let main_chain_block = self
                .mainchain_height_to_header
                .insert(&current_height, &current_block_hash);
            self.mainchain_header_to_height
                .insert(&current_block_hash, &current_height);

            // A main chain block displaced by the fork stays in the pool: it becomes a part
            // of the fork the old main chain turns into
            if let Some(current_main_chain_blockhash) = main_chain_block {
                self.mainchain_header_to_height
                    .remove(&current_main_chain_blockhash);
            }

            // Switch iterator cursor to the previous block in fork
            fork_header_cursor = self
                .headers_pool
                .get(&prev_block_hash)
                .unwrap_or_else(|| env::panic_str("previous fork block should be there"));
        }

        // The loop above stops at the lowest common ancestor of the fork and the old main chain
        let lca_height = fork_header_cursor.block_height;

        // The fork is the main chain now, and the old main chain is a fork branching off the LCA
        self.remove_fork_tip(&fork_tip_hash, fork_tip_height);
        self.update_forks_lca(lca_height);
        self.insert_fork_tip(
            lca_height,
            old_main_chain_tip_hash,
            last_main_chain_block_height,
        );

        // Updating tip of the new main chain
        self.mainchain_tip_blockhash = fork_tip_hash;
    }

    /// Stores parsed block header and meta information
    fn store_block_header(&mut self, header: &ExtendedHeader) {
        self.mainchain_height_to_header
            .insert(&header.block_height, &header.block_hash);
        self.mainchain_header_to_height
            .insert(&header.block_hash, &header.block_height);
        self.headers_pool.insert(&header.block_hash, header);
    }

    /// Remove block header and meta information
    fn remove_block_header(&mut self, header_block_hash: &H256) {
        self.mainchain_header_to_height.remove(header_block_hash);
        self.headers_pool.remove(header_block_hash);
    }

    /// Stores and handles fork submissions
    fn store_fork_header(&mut self, header: &ExtendedHeader) {
        self.headers_pool.insert(&header.block_hash, header);
    }

    /// Registers a fork block in `forks_tips`: either moves the tip of the fork the block
    /// extends, or inserts a new tip keeping the list sorted by `lca_height`
    fn update_forks_tips(&mut self, header: &ExtendedHeader, main_chain_tip_height: u64) {
        // Resubmitting a block, a main chain one included, goes through this very code path
        if self.headers_pool.contains_key(&header.block_hash) {
            return;
        }

        let prev_block_hash = &header.block_header.prev_block_hash;

        let (lca_height, extended_tip) =
            if let Some(lca_height) = self.mainchain_header_to_height.get(prev_block_hash) {
                (lca_height, None)
            } else if let Some(index) =
                self.find_fork_tip(prev_block_hash, header.block_height.saturating_sub(1))
            {
                (self.forks_tips[index].lca_height, Some(index))
            } else {
                (
                    self.lca_height_by_hash(prev_block_hash.clone())
                        .unwrap_or_else(|| env::panic_str("ERR_FORK_LCA_NOT_FOUND")),
                    None,
                )
            };

        // Complementary to the forks GC criterion, so an accepted fork is never outdated at once
        require!(
            main_chain_tip_height.saturating_sub(lca_height) <= self.max_reorg,
            "ERR_FORK_TOO_DEEP"
        );

        if let Some(index) = extended_tip {
            self.forks_tips[index].tip_hash = header.block_hash.clone();
            self.forks_tips[index].tip_height = header.block_height;
            self.recompute_prefix_max_tip_height(index);
        } else {
            self.insert_fork_tip(lca_height, header.block_hash.clone(), header.block_height);
        }
    }

    fn insert_fork_tip(&mut self, lca_height: u64, tip_hash: H256, tip_height: u64) {
        // Insert after the tips with an equal LCA, so that the common case is a plain append
        let index = self
            .forks_tips
            .partition_point(|fork_tip| fork_tip.lca_height <= lca_height);

        self.forks_tips.insert(
            index,
            ForkTip {
                lca_height,
                tip_hash,
                tip_height,
                prefix_max_tip_height: 0,
            },
        );
        self.recompute_prefix_max_tip_height(index);
    }

    fn remove_fork_tip(&mut self, tip_hash: &H256, tip_height: u64) {
        if let Some(index) = self.find_fork_tip(tip_hash, tip_height) {
            self.forks_tips.remove(index);
            self.recompute_prefix_max_tip_height(index);
        }
    }

    /// Tip height of the most dangerous fork for a block at `block_height`: the highest tip
    /// among the forks branching off below the block, i.e. the ones not containing it
    fn dangerous_fork_tip_height(&self, block_height: u64) -> Option<u64> {
        let dangerous_forks = self
            .forks_tips
            .partition_point(|fork_tip| fork_tip.lca_height < block_height);

        dangerous_forks
            .checked_sub(1)
            .map(|index| self.forks_tips[index].prefix_max_tip_height)
    }

    /// Looks up the tip `tip_hash` of height `tip_height`.
    ///
    /// Searches backwards, as the recently added tips are at the end, and stops once
    /// `prefix_max_tip_height` drops below the height the tip must have
    fn find_fork_tip(&self, tip_hash: &H256, tip_height: u64) -> Option<usize> {
        for (index, fork_tip) in self.forks_tips.iter().enumerate().rev() {
            if fork_tip.prefix_max_tip_height < tip_height {
                return None;
            }

            if fork_tip.tip_hash == *tip_hash {
                return Some(index);
            }
        }

        None
    }

    /// Removes up to `limit` blocks of the fork, walking down from its tip. Returns how many
    /// were removed and the highest block left, if the fork is not removed completely
    fn remove_fork_blocks(&mut self, fork_tip: &ForkTip, limit: u64) -> (u64, Option<(H256, u64)>) {
        let mut block_hash = fork_tip.tip_hash.clone();
        let mut block_height = fork_tip.tip_height;
        let mut removed = 0;

        while removed < limit {
            let Some(header) = self.headers_pool.get(&block_hash) else {
                return (removed, None);
            };

            // Stop at the LCA, and never touch the main chain even if the stored LCA is stale
            if header.block_height <= fork_tip.lca_height
                || self.mainchain_header_to_height.contains_key(&block_hash)
            {
                return (removed, None);
            }

            self.headers_pool.remove(&block_hash);
            removed += 1;
            block_hash = header.block_header.prev_block_hash;
            block_height = header.block_height - 1;
        }

        if block_height > fork_tip.lca_height {
            (removed, Some((block_hash, block_height)))
        } else {
            (removed, None)
        }
    }

    /// Height of the lowest common ancestor of the fork the block belongs to and the main
    /// chain, found by walking the fork down to the first block of the main chain.
    /// `None` if the walk runs into a block which is not stored anymore
    fn lca_height_by_hash(&self, mut block_hash: H256) -> Option<u64> {
        loop {
            if let Some(height) = self.mainchain_header_to_height.get(&block_hash) {
                return Some(height);
            }

            block_hash = self
                .headers_pool
                .get(&block_hash)?
                .block_header
                .prev_block_hash;
        }
    }

    /// Restores the LCA of the tracked forks after a reorg at `reorg_height`: the forks which
    /// branched off the replaced part of the main chain branch off the reorg point now
    fn update_forks_lca(&mut self, reorg_height: u64) {
        // The forks branching off below the reorg point are not affected
        let from = self
            .forks_tips
            .partition_point(|fork_tip| fork_tip.lca_height < reorg_height);

        // Reinserting keeps the list sorted, as the updated LCA may be both lower and higher
        for fork_tip in self.forks_tips.split_off(from) {
            let lca_height = if fork_tip.lca_height > reorg_height {
                reorg_height
            } else {
                // The reorg point itself or the fork which won: only a walk can tell
                self.lca_height_by_hash(fork_tip.tip_hash.clone())
                    .unwrap_or(reorg_height)
            };

            self.insert_fork_tip(lca_height, fork_tip.tip_hash, fork_tip.tip_height);
        }
    }

    /// Restores the `prefix_max_tip_height` invariant broken by the change at `changed_index`
    fn recompute_prefix_max_tip_height(&mut self, changed_index: usize) {
        let mut prefix_max = if changed_index == 0 {
            0
        } else {
            self.forks_tips[changed_index - 1].prefix_max_tip_height
        };

        for (index, fork_tip) in self.forks_tips.iter_mut().enumerate().skip(changed_index) {
            prefix_max = std::cmp::max(prefix_max, fork_tip.tip_height);

            // The following tips depend on the stored maximum only, so they are correct too
            if index > changed_index && fork_tip.prefix_max_tip_height == prefix_max {
                return;
            }

            fork_tip.prefix_max_tip_height = prefix_max;
        }
    }
}

impl BlocksGetter for BtcLightClient {
    fn get_prev_header(&self, current_header: &LightHeader) -> ExtendedHeader {
        self.headers_pool
            .get(&current_header.prev_block_hash)
            .unwrap_or_else(|| env::panic_str("PrevBlockNotFound"))
    }

    fn get_header_by_height(&self, height: u64) -> ExtendedHeader {
        self.mainchain_height_to_header
            .get(&height)
            .and_then(|hash| self.headers_pool.get(&hash))
            .unwrap_or_else(|| env::panic_str(ERR_KEY_NOT_EXIST))
    }
}

mod migrate {
    use crate::{
        borsh, env, log, near, BorshDeserialize, BorshSerialize, BtcLightClient, BtcLightClientExt,
        ExtendedHeader, LookupMap, Network, PanicOnDefault, H256,
    };

    /// State layout before `bootstrap_end_height` was added.
    #[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
    pub struct BtcLightClientV3 {
        mainchain_height_to_header: LookupMap<u64, H256>,
        mainchain_header_to_height: LookupMap<H256, u64>,
        mainchain_tip_blockhash: H256,
        mainchain_initial_blockhash: H256,
        headers_pool: LookupMap<H256, ExtendedHeader>,
        skip_pow_verification: bool,
        gc_threshold: u64,
        network: Network,
    }

    /// State layout used between #101 and #116, which contained the
    /// `used_aux_parent_blocks` field in all chain builds.
    #[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
    pub struct BtcLightClientV2 {
        mainchain_height_to_header: LookupMap<u64, H256>,
        mainchain_header_to_height: LookupMap<H256, u64>,
        mainchain_tip_blockhash: H256,
        mainchain_initial_blockhash: H256,
        headers_pool: LookupMap<H256, ExtendedHeader>,
        skip_pow_verification: bool,
        gc_threshold: u64,
        used_aux_parent_blocks: near_sdk::collections::LookupSet<H256>,
        network: Network,
    }

    /// State layout after `bootstrap_end_height` was added and before the fork tracking,
    /// i.e. the current layout without the `max_reorg` and `forks_tips` fields.
    #[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
    pub struct BtcLightClientV4 {
        mainchain_height_to_header: LookupMap<u64, H256>,
        mainchain_header_to_height: LookupMap<H256, u64>,
        mainchain_tip_blockhash: H256,
        mainchain_initial_blockhash: H256,
        headers_pool: LookupMap<H256, ExtendedHeader>,
        skip_pow_verification: bool,
        gc_threshold: u64,
        network: Network,
        bootstrap_end_height: u64,
    }

    #[near]
    impl BtcLightClient {
        /// Migrates the contract state to the current `BtcLightClient` version.
        ///
        /// The stored state variant is detected automatically. Borsh requires the
        /// whole buffer to be consumed, so exactly one of the layouts can parse:
        /// * current layout: returned unchanged (re-running `migrate` is a no-op)
        /// * `BtcLightClientV4` (before fork tracking): starts with an empty list of fork
        ///   tips and the default `max_reorg`
        /// * `BtcLightClientV3`: adds `bootstrap_end_height = 0`, an empty list of fork tips
        ///   and the default `max_reorg`
        /// * `BtcLightClientV2` (#101..#116): drops `used_aux_parent_blocks`;
        ///   `network` is carried over from the old state, `bootstrap_end_height = 0`,
        ///   an empty list of fork tips and the default `max_reorg`
        ///
        /// Note: any entries stored under the dropped `LookupSet` prefix are left
        /// orphaned in storage. They are only present on Dogecoin deployments;
        /// other chains never wrote to the set.
        ///
        /// # Panics
        /// This function will panic if no state is found in storage, or it
        /// matches none of the known layouts.
        #[private]
        #[init(ignore_state)]
        #[must_use]
        pub fn migrate() -> Self {
            let raw_state = env::storage_read(b"STATE")
                .unwrap_or_else(|| env::panic_str("contract state not found"));

            if let Ok(state) = <Self as BorshDeserialize>::try_from_slice(&raw_state) {
                log!("state is already in the current layout");
                return state;
            }

            if let Ok(old_state) = BtcLightClientV4::try_from_slice(&raw_state) {
                log!("migrating state from the V4 layout");
                return Self {
                    mainchain_height_to_header: old_state.mainchain_height_to_header,
                    mainchain_header_to_height: old_state.mainchain_header_to_height,
                    mainchain_tip_blockhash: old_state.mainchain_tip_blockhash,
                    mainchain_initial_blockhash: old_state.mainchain_initial_blockhash,
                    headers_pool: old_state.headers_pool,
                    skip_pow_verification: old_state.skip_pow_verification,
                    gc_threshold: old_state.gc_threshold,
                    max_reorg: std::cmp::min(crate::DEFAULT_MAX_REORG, old_state.gc_threshold),
                    network: old_state.network,
                    bootstrap_end_height: old_state.bootstrap_end_height,
                    forks_tips: Vec::new(),
                };
            }

            if let Ok(old_state) = BtcLightClientV3::try_from_slice(&raw_state) {
                log!("migrating state from the V3 layout");
                return Self {
                    mainchain_height_to_header: old_state.mainchain_height_to_header,
                    mainchain_header_to_height: old_state.mainchain_header_to_height,
                    mainchain_tip_blockhash: old_state.mainchain_tip_blockhash,
                    mainchain_initial_blockhash: old_state.mainchain_initial_blockhash,
                    headers_pool: old_state.headers_pool,
                    skip_pow_verification: old_state.skip_pow_verification,
                    gc_threshold: old_state.gc_threshold,
                    max_reorg: std::cmp::min(crate::DEFAULT_MAX_REORG, old_state.gc_threshold),
                    network: old_state.network,
                    bootstrap_end_height: 0,
                    forks_tips: Vec::new(),
                };
            }

            if let Ok(old_state) = BtcLightClientV2::try_from_slice(&raw_state) {
                log!("migrating state from the V2 layout");
                return Self {
                    mainchain_height_to_header: old_state.mainchain_height_to_header,
                    mainchain_header_to_height: old_state.mainchain_header_to_height,
                    mainchain_tip_blockhash: old_state.mainchain_tip_blockhash,
                    mainchain_initial_blockhash: old_state.mainchain_initial_blockhash,
                    headers_pool: old_state.headers_pool,
                    skip_pow_verification: old_state.skip_pow_verification,
                    gc_threshold: old_state.gc_threshold,
                    max_reorg: std::cmp::min(crate::DEFAULT_MAX_REORG, old_state.gc_threshold),
                    network: old_state.network,
                    bootstrap_end_height: 0,
                    forks_tips: Vec::new(),
                };
            }

            env::panic_str("contract state matches no known layout")
        }
    }
}

/*
 * The rest of this file holds the inline tests for the code above
 * Learn more about Rust tests: https://doc.rust-lang.org/book/ch11-01-writing-tests.html
 */
#[cfg(test)]
#[cfg(feature = "bitcoin")]
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

    // Returns 12 real mainnet block headers from heights 685440-685451.
    // All have bits=386752379 and version >= 4; used to pre-populate the chain
    // so that MTP (median time past) can be computed when submitting height 685452.
    fn real_block_headers_685440_to_685451() -> Vec<Header> {
        fn h(version: i32, prev: &str, merkle: &str, time: u32, nonce: u32) -> Header {
            serde_json::from_value(serde_json::json!({
                "version": version,
                "prev_block_hash": prev,
                "merkle_root": merkle,
                "time": time,
                "bits": 386752379u32,
                "nonce": nonce,
            }))
            .unwrap()
        }
        vec![
            h(
                805298180,
                "00000000000000000006248c28751a176336f5c070f901dc86df190c391d761d",
                "534e13aa090e6615a2a6610f49b42ca9caa93f3ce2ca33735ca11444d6705424",
                1622337521,
                1876340370,
            ),
            h(
                536928260,
                "000000000000000000016f0484972d135afba541c837d0c07c1530ffeee293cd",
                "1f3e2b319668356eba575e5e9aa4b742a7f79a1419e3cf58d81430aaf4a66458",
                1622338991,
                3480744496,
            ),
            h(
                671080452,
                "00000000000000000003fb78da0a99e751b6cf726723a99b9ca2dc6e8bb45544",
                "a3adf4e20aa658bc7f74b9674ab190252b161a30d368ad90892876060b7c35a0",
                1622339181,
                1405379453,
            ),
            h(
                536870916,
                "00000000000000000009165c5600f52cb7436b40f3ad48e996de63d63e1a124e",
                "f13831ddb7dab19aca9c95b9841bf7100752a86e192c6744692196c62b2bf906",
                1622340913,
                1822192411,
            ),
            h(
                536870916,
                "0000000000000000000172c10f510c380eb7f39f57dfee12ca4384a1d594e58b",
                "76d7acbec78d65b40809d3159c5c9e6e5c690a19e60b17a60e050e9c27376905",
                1622341014,
                4080508367,
            ),
            h(
                545259524,
                "0000000000000000000692c97d64558f78f7868ed8f6bfeaeccdf3d1aa8bffef",
                "7a547eb44976320a703e35afeafa1d8b2820ab90625ca51063e678dfcec713bc",
                1622341455,
                81234816,
            ),
            h(
                551550980,
                "00000000000000000007b9eefcda99e224435a7a9957dbea4ed980d6dae31947",
                "f9c2771b05a32f57e3fa613a806640726bec49b9428341b378bf9172a8272668",
                1622342223,
                3439579250,
            ),
            h(
                939515908,
                "00000000000000000002b3e7277440332e1332e88e524ec68104df590842001e",
                "28e1ee6ba3297e377ea84c9e7871e4e8b6428e64585ef99696ad9d239e478f08",
                1622342446,
                223964937,
            ),
            h(
                805298180,
                "0000000000000000000108586d66d4cedb85bb95c7ed7f225cbf035120b4e8e3",
                "344d55da3a31e027ff633bbe2e67c1418a375d4a321f3a301a22c0d23d95fb1f",
                1622342844,
                624362111,
            ),
            h(
                545259524,
                "00000000000000000002c37de19d48c2d15dfd528d95c33ebb0dd81f8c6e30a8",
                "eb294f1daf84a4ea942cb62606ab385f229e89703b8efe02714b4a62d8181ee9",
                1622344117,
                3908216100,
            ),
            h(
                1073733636,
                "00000000000000000004334c161711f1f213996cbeb53051a0759065ade81e8a",
                "a7f923d2034b9a737e9e31c479e5d21769c956d6e583c2ad369e3609d20db23f",
                1622344380,
                392620416,
            ),
            h(
                549453828,
                "00000000000000000002f45558f6bc1e8bd97ce127fe8217d69f04d4938265b2",
                "e5cddebf98270b2cc1b6fd931bf9dabd1c815b946d39ced94f066aca8429d419",
                1622344394,
                2990099038,
            ),
        ]
    }

    // Real mainnet block at height 685452.
    fn block_685452_header() -> Header {
        serde_json::from_value(serde_json::json!({
            "version": 939515908i32,
            "prev_block_hash": "000000000000000000041f7db3a18612b7439c91398f21bd816fb3c93c74099d",
            "merkle_root": "6dc8856dc7b1c460f2ca355f9ef2a9f2ad84f882404436e51ec8c970e5e248f4",
            "time": 1622344447u32,
            "bits": 386752379u32,
            "nonce": 820693939u32,
        }))
        .unwrap()
    }

    // Initializes with real mainnet block 685440 and bootstraps 685441-685451, skip_pow=false.
    // Height 685440 is a difficulty-adjustment boundary (685440 % 2016 == 0).
    fn init_with_real_blocks() -> BtcLightClient {
        let blocks = real_block_headers_685440_to_685451();
        let mut contract = BtcLightClient::init(InitArgs {
            network: Network::Mainnet,
            genesis_block_hash: blocks[0].block_hash(),
            genesis_block_height: 685440,
            skip_pow_verification: false,
            gc_threshold: 1000,
            genesis_block: blocks[0].clone(),
        });
        for header in &blocks[1..] {
            contract.submit_bootstrap_block_header(header.clone());
        }
        contract.finish_bootstrap_if_done();
        contract
    }

    fn grant_role(contract: &mut BtcLightClient, role: Role, account_id: &near_sdk::AccountId) {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new()
            .predecessor_account_id(env::current_account_id())
            .build());
        contract.acl_grant_role(<&str>::from(role).to_owned(), account_id.clone());
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new()
            .predecessor_account_id(account_id.clone())
            .attached_deposit(NearToken::from_near(1))
            .build());
    }

    fn get_default_init_args() -> InitArgs {
        let genesis_block = genesis_block_header();
        InitArgs {
            network: Network::Mainnet,
            genesis_block_hash: genesis_block.block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: false,
            gc_threshold: 3,
            genesis_block,
        }
    }

    fn get_default_init_args_with_skip_pow() -> InitArgs {
        InitArgs {
            skip_pow_verification: true,
            ..get_default_init_args()
        }
    }

    #[test]
    #[should_panic(expected = "block should have correct pow")]
    fn test_pow_validator_works_correctly_for_wrong_block() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new()
            .block_timestamp(1_622_344_600_000_000_000u64)
            .build());
        let mut header = block_685452_header();
        header.nonce += 1; // tampered nonce → hash won't satisfy PoW target
        let mut contract = init_with_real_blocks();
        contract.submit_block_header(header, contract.skip_pow_verification);
    }

    #[test]
    fn test_pow_validator_works_correctly_for_correct_block() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new()
            .block_timestamp(1_622_344_600_000_000_000u64)
            .build());
        let header = block_685452_header();
        let mut contract = init_with_real_blocks();
        contract.submit_block_header(header.clone(), contract.skip_pow_verification);

        let received_header = contract.get_last_block_header();

        let w = work_from_bits(386752379);
        let mut expected_chain_work = w;
        for _ in 0..12 {
            let (new_w, _) = expected_chain_work.overflowing_add(w);
            expected_chain_work = new_w;
        }

        assert_eq!(
            received_header,
            ExtendedHeader {
                block_header: header,
                block_hash: decode_hex(
                    "00000000000000000001c1b436262471ec926e6cff522ec048c912e26ebba6cc"
                ),
                chain_work: expected_chain_work,
                block_height: 685452,
            }
        );
    }

    #[test]
    fn test_saving_mainchain_block_header() {
        let header = block_header_example();

        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        contract.submit_block_header(header.clone(), contract.skip_pow_verification);

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
                block_height: 1,
            }
        );
    }

    /// Genesis plus `count` properly chained blocks, i.e. a main chain without any forks
    fn make_chained_submit_blocks(count: u32) -> Vec<Header> {
        let genesis = genesis_block_header();
        let mut prev_block_hash = genesis.block_hash();
        let mut blocks = vec![genesis];

        for nonce in 0..count {
            let header = make_fork_block(&prev_block_hash, nonce);
            prev_block_hash = header.block_hash();
            blocks.push(header);
        }

        blocks
    }

    fn make_fork_block(prev_block_hash: &H256, nonce: u32) -> Header {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "prev_block_hash": prev_block_hash.to_string(),
            "merkle_root": "0000000000000000000000000000000000000000000000000000000000000000",
            "time": 1_231_006_506u32 + nonce,
            "bits": 0x207f_ffffu32,
            "nonce": nonce,
        }))
        .unwrap()
    }

    /// A contract whose main chain tip is at height 11
    fn init_contract_with_chained_blocks() -> BtcLightClient {
        let blocks = make_chained_submit_blocks(11);

        let mut contract = BtcLightClient::init(InitArgs {
            network: Network::Mainnet,
            genesis_block_hash: blocks[0].block_hash(),
            genesis_block_height: 0,
            skip_pow_verification: true,
            gc_threshold: 100,
            genesis_block: blocks[0].clone(),
        });
        for header in &blocks[1..] {
            contract.submit_block_header(header.clone(), contract.skip_pow_verification);
        }
        contract
    }

    fn submit_fork_block(
        contract: &mut BtcLightClient,
        prev_block_hash: &H256,
        nonce: u32,
    ) -> H256 {
        let header = make_fork_block(prev_block_hash, nonce);
        let block_hash = header.block_hash();
        contract.submit_block_header(header, contract.skip_pow_verification);
        block_hash
    }

    #[test]
    fn test_main_chain_blocks_do_not_create_fork_tips() {
        let mut contract = init_contract_with_chained_blocks();
        assert!(contract.forks_tips.is_empty());

        // Resubmitting a main chain block goes through the fork submission path
        let block_5 = contract.get_block_hash_by_height(5).unwrap();
        let header = contract.headers_pool.get(&block_5).unwrap();
        assert_eq!(header.block_height, 5);

        contract.submit_block_header(header.block_header, contract.skip_pow_verification);

        assert!(contract.forks_tips.is_empty());
    }

    #[test]
    fn test_fork_off_the_main_chain_creates_a_tip() {
        let mut contract = init_contract_with_chained_blocks();
        let block_5 = contract.get_block_hash_by_height(5).unwrap();
        let fork_tip_hash = submit_fork_block(&mut contract, &block_5, 100);

        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 5,
                tip_hash: fork_tip_hash,
                tip_height: 6,
                prefix_max_tip_height: 6,
            }]
        );
    }

    #[test]
    fn test_fork_extending_a_known_tip_moves_it() {
        let mut contract = init_contract_with_chained_blocks();
        let block_5 = contract.get_block_hash_by_height(5).unwrap();
        let first_fork_block = submit_fork_block(&mut contract, &block_5, 100);
        let fork_tip_hash = submit_fork_block(&mut contract, &first_fork_block, 101);

        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 5,
                tip_hash: fork_tip_hash,
                tip_height: 7,
                prefix_max_tip_height: 7,
            }]
        );
    }

    #[test]
    fn test_fork_branching_off_the_middle_of_a_fork_creates_a_tip() {
        let mut contract = init_contract_with_chained_blocks();
        let block_5 = contract.get_block_hash_by_height(5).unwrap();
        let first_fork_block = submit_fork_block(&mut contract, &block_5, 100);
        let first_tip = submit_fork_block(&mut contract, &first_fork_block, 101);
        // Branches off a block which is neither a main chain block nor a known tip
        let second_tip = submit_fork_block(&mut contract, &first_fork_block, 102);

        assert_eq!(
            contract.forks_tips,
            vec![
                ForkTip {
                    lca_height: 5,
                    tip_hash: first_tip,
                    tip_height: 7,
                    prefix_max_tip_height: 7,
                },
                ForkTip {
                    lca_height: 5,
                    tip_hash: second_tip,
                    tip_height: 7,
                    prefix_max_tip_height: 7,
                }
            ]
        );
    }

    #[test]
    fn test_fork_tips_are_sorted_by_lca_height() {
        let mut contract = init_contract_with_chained_blocks();
        let mut tips = vec![];

        // Submitted out of order: the tip with the deepest LCA comes last
        for (nonce, lca_height) in [(100, 8), (101, 3), (102, 5)] {
            let block_hash = contract.get_block_hash_by_height(lca_height).unwrap();
            tips.push((
                lca_height,
                submit_fork_block(&mut contract, &block_hash, nonce),
            ));
        }

        tips.sort_by_key(|(lca_height, _)| *lca_height);
        assert_eq!(
            contract.forks_tips,
            tips.into_iter()
                .map(|(lca_height, tip_hash)| ForkTip {
                    lca_height,
                    tip_hash,
                    tip_height: lca_height + 1,
                    prefix_max_tip_height: lca_height + 1,
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_extending_the_oldest_tip_is_still_found() {
        let mut contract = init_contract_with_chained_blocks();
        let block_3 = contract.get_block_hash_by_height(3).unwrap();
        let block_8 = contract.get_block_hash_by_height(8).unwrap();

        let old_fork_block = submit_fork_block(&mut contract, &block_3, 100);
        let recent_tip = submit_fork_block(&mut contract, &block_8, 101);
        // The extended tip is the first in the list, behind a tip with a higher prefix maximum
        let old_fork_tip = submit_fork_block(&mut contract, &old_fork_block, 102);

        assert_eq!(
            contract.forks_tips,
            vec![
                ForkTip {
                    lca_height: 3,
                    tip_hash: old_fork_tip,
                    tip_height: 5,
                    prefix_max_tip_height: 5,
                },
                ForkTip {
                    lca_height: 8,
                    tip_hash: recent_tip,
                    tip_height: 9,
                    prefix_max_tip_height: 9,
                }
            ]
        );
    }

    #[test]
    fn test_fork_of_a_fork_next_to_an_older_tip() {
        let mut contract = init_contract_with_chained_blocks();
        let block_3 = contract.get_block_hash_by_height(3).unwrap();
        let block_8 = contract.get_block_hash_by_height(8).unwrap();

        let old_fork_tip = submit_fork_block(&mut contract, &block_3, 100);
        let branching_block = submit_fork_block(&mut contract, &block_8, 101);
        let first_tip = submit_fork_block(&mut contract, &branching_block, 102);
        // The tip search stops before the older tip, so the LCA is computed by walking
        let second_tip = submit_fork_block(&mut contract, &branching_block, 103);

        assert_eq!(
            contract.forks_tips,
            vec![
                ForkTip {
                    lca_height: 3,
                    tip_hash: old_fork_tip,
                    tip_height: 4,
                    prefix_max_tip_height: 4,
                },
                ForkTip {
                    lca_height: 8,
                    tip_hash: first_tip,
                    tip_height: 10,
                    prefix_max_tip_height: 10,
                },
                ForkTip {
                    lca_height: 8,
                    tip_hash: second_tip,
                    tip_height: 10,
                    prefix_max_tip_height: 10,
                }
            ]
        );
    }

    #[test]
    fn test_reorg_registers_the_old_main_chain_as_a_fork() {
        let mut contract = init_contract_with_chained_blocks();
        let old_main_chain_tip = contract.get_block_hash_by_height(11).unwrap();
        let mut prev_block_hash = contract.get_block_hash_by_height(8).unwrap();

        // Four fork blocks outweigh the three main chain blocks above height 8
        for nonce in 100..104 {
            prev_block_hash = submit_fork_block(&mut contract, &prev_block_hash, nonce);
        }

        assert_eq!(contract.mainchain_tip_blockhash, prev_block_hash);
        assert_eq!(contract.get_last_block_height(), 12);
        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 8,
                tip_hash: old_main_chain_tip.clone(),
                tip_height: 11,
                prefix_max_tip_height: 11,
            }]
        );

        // The old main chain is kept in the pool, but it is not the main chain anymore
        assert!(contract.headers_pool.contains_key(&old_main_chain_tip));
        assert_eq!(contract.get_height_by_block_hash(old_main_chain_tip), None);
    }

    #[test]
    fn test_reorg_to_a_shorter_fork_keeps_the_old_main_chain() {
        let mut contract = init_contract_with_chained_blocks();
        let old_main_chain_tip = contract.get_block_hash_by_height(11).unwrap();
        let old_main_chain_block = contract.get_block_hash_by_height(5).unwrap();

        // A single block of the difficulty-1 target outweighs the whole easy main chain
        let header = block_header_example();
        contract.submit_block_header(header.clone(), contract.skip_pow_verification);

        assert_eq!(contract.mainchain_tip_blockhash, header.block_hash());
        assert_eq!(contract.get_last_block_height(), 1);
        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 0,
                tip_hash: old_main_chain_tip.clone(),
                tip_height: 11,
                prefix_max_tip_height: 11,
            }]
        );

        // The blocks above the fork tip are kept as well, and no height maps to them anymore
        assert!(contract.headers_pool.contains_key(&old_main_chain_tip));
        assert!(contract.headers_pool.contains_key(&old_main_chain_block));
        assert!(contract.get_block_hash_by_height(5).is_none());
        assert_eq!(
            contract.get_height_by_block_hash(old_main_chain_block),
            None
        );
    }

    #[test]
    fn test_dangerous_fork_tip_height_ignores_the_forks_containing_the_block() {
        let mut contract = init_contract_with_chained_blocks();
        assert_eq!(contract.dangerous_fork_tip_height(5), None);

        let block_3 = contract.get_block_hash_by_height(3).unwrap();
        submit_fork_block(&mut contract, &block_3, 100);

        // The LCA belongs to both chains, so the blocks up to it are not endangered
        assert_eq!(contract.dangerous_fork_tip_height(3), None);
        assert_eq!(contract.dangerous_fork_tip_height(4), Some(4));
        assert_eq!(contract.dangerous_fork_tip_height(11), Some(4));
    }

    #[test]
    fn test_dangerous_fork_tip_height_takes_the_highest_tip() {
        let mut contract = init_contract_with_chained_blocks();
        let block_3 = contract.get_block_hash_by_height(3).unwrap();
        let block_8 = contract.get_block_hash_by_height(8).unwrap();

        let old_fork_block = submit_fork_block(&mut contract, &block_3, 100);
        submit_fork_block(&mut contract, &old_fork_block, 101);
        submit_fork_block(&mut contract, &block_8, 102);

        // Only the fork branching off height 3 endangers the block at height 4
        assert_eq!(contract.dangerous_fork_tip_height(4), Some(5));
        // Both forks do, and the highest tip of the two is reported
        assert_eq!(contract.dangerous_fork_tip_height(9), Some(9));
    }

    #[test]
    fn test_forks_gc_removes_the_forks_branching_off_too_deep() {
        let mut contract = init_contract_with_chained_blocks();

        let block_2 = contract.get_block_hash_by_height(2).unwrap();
        let block_9 = contract.get_block_hash_by_height(9).unwrap();
        let outdated_fork_tip = submit_fork_block(&mut contract, &block_2, 100);
        let recent_fork_tip = submit_fork_block(&mut contract, &block_9, 101);

        contract.max_reorg = 3;
        contract.run_forks_gc(100);

        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 9,
                tip_hash: recent_fork_tip.clone(),
                tip_height: 10,
                prefix_max_tip_height: 10,
            }]
        );
        assert!(!contract.headers_pool.contains_key(&outdated_fork_tip));
        assert!(contract.headers_pool.contains_key(&recent_fork_tip));
        // The LCA is a main chain block and is kept
        assert!(contract.headers_pool.contains_key(&block_2));
    }

    #[test]
    fn test_forks_gc_resumes_a_fork_removed_in_batches() {
        let mut contract = init_contract_with_chained_blocks();

        let block_2 = contract.get_block_hash_by_height(2).unwrap();
        let mut fork_blocks = vec![];
        let mut prev_block_hash = block_2.clone();
        for nonce in 100..103 {
            prev_block_hash = submit_fork_block(&mut contract, &prev_block_hash, nonce);
            fork_blocks.push(prev_block_hash.clone());
        }

        contract.max_reorg = 3;
        contract.run_forks_gc(2);

        assert!(!contract.headers_pool.contains_key(&fork_blocks[2]));
        assert!(!contract.headers_pool.contains_key(&fork_blocks[1]));
        assert!(contract.headers_pool.contains_key(&fork_blocks[0]));
        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 2,
                tip_hash: fork_blocks[0].clone(),
                tip_height: 3,
                prefix_max_tip_height: 3,
            }]
        );

        contract.run_forks_gc(2);

        assert!(!contract.headers_pool.contains_key(&fork_blocks[0]));
        assert!(contract.forks_tips.is_empty());
        assert!(contract.headers_pool.contains_key(&block_2));
    }

    #[test]
    fn test_forks_gc_removes_a_fork_with_two_tips() {
        let mut contract = init_contract_with_chained_blocks();

        let block_2 = contract.get_block_hash_by_height(2).unwrap();
        let shared_block = submit_fork_block(&mut contract, &block_2, 100);
        let first_tip = submit_fork_block(&mut contract, &shared_block, 101);
        let second_tip = submit_fork_block(&mut contract, &shared_block, 102);

        contract.max_reorg = 3;
        contract.run_forks_gc(100);

        assert!(contract.forks_tips.is_empty());
        for block_hash in [&shared_block, &first_tip, &second_tip] {
            assert!(!contract.headers_pool.contains_key(block_hash));
        }
        assert!(contract.headers_pool.contains_key(&block_2));
    }

    #[test]
    fn test_forks_gc_keeps_the_forks_within_max_reorg() {
        let mut contract = init_contract_with_chained_blocks();
        contract.max_reorg = 11;

        let block_0 = contract.get_block_hash_by_height(0).unwrap();
        let fork_tip = submit_fork_block(&mut contract, &block_0, 100);
        let forks_tips_before = contract.forks_tips.clone();

        contract.run_forks_gc(100);

        assert_eq!(contract.forks_tips, forks_tips_before);
        assert!(contract.headers_pool.contains_key(&fork_tip));
    }

    #[test]
    #[should_panic(expected = "ERR_FORK_TOO_DEEP")]
    fn test_fork_branching_off_deeper_than_max_reorg_is_rejected() {
        let mut contract = init_contract_with_chained_blocks();
        contract.max_reorg = 2;

        let block_8 = contract.get_block_hash_by_height(8).unwrap();
        submit_fork_block(&mut contract, &block_8, 100);
    }

    #[test]
    fn test_fork_branching_off_exactly_at_max_reorg_is_accepted() {
        let mut contract = init_contract_with_chained_blocks();
        contract.max_reorg = 3;

        // The main chain tip is at height 11, so this fork is exactly at the horizon
        let block_8 = contract.get_block_hash_by_height(8).unwrap();
        let fork_tip = submit_fork_block(&mut contract, &block_8, 100);

        contract.run_forks_gc(100);

        assert!(contract.headers_pool.contains_key(&fork_tip));
        assert_eq!(contract.forks_tips.len(), 1);
    }

    #[test]
    #[should_panic(expected = "ERR_FORK_TOO_DEEP")]
    fn test_extending_a_fork_below_max_reorg_is_rejected() {
        let mut contract = init_contract_with_chained_blocks();
        let block_8 = contract.get_block_hash_by_height(8).unwrap();
        let fork_block = submit_fork_block(&mut contract, &block_8, 100);

        contract.max_reorg = 1;
        submit_fork_block(&mut contract, &fork_block, 101);
    }

    #[test]
    #[should_panic(expected = "ERR_FORK_LCA_NOT_FOUND")]
    fn test_fork_with_a_collected_lca_is_rejected() {
        let mut contract = init_contract_with_chained_blocks();
        let block_5 = contract.get_block_hash_by_height(5).unwrap();
        let fork_block = submit_fork_block(&mut contract, &block_5, 100);
        submit_fork_block(&mut contract, &fork_block, 101);

        // The main chain GC removes the LCA of the fork
        contract.gc_threshold = 2;
        contract.run_mainchain_gc(20);

        // Branching off the middle of the fork has to walk down to the missing LCA
        submit_fork_block(&mut contract, &fork_block, 102);
    }

    #[test]
    fn test_reorg_updates_the_lca_of_the_other_forks() {
        let mut contract = init_contract_with_chained_blocks();
        let old_main_chain_tip = contract.get_block_hash_by_height(11).unwrap();
        let block_3 = contract.get_block_hash_by_height(3).unwrap();
        let block_8 = contract.get_block_hash_by_height(8).unwrap();
        let block_10 = contract.get_block_hash_by_height(10).unwrap();

        // A branch below the reorg point and a branch off the old main chain above it
        let untouched_tip = submit_fork_block(&mut contract, &block_3, 100);
        let old_chain_branch_tip = submit_fork_block(&mut contract, &block_10, 101);

        // The fork about to win, with a branch of its own hanging off its middle
        let winner_1 = submit_fork_block(&mut contract, &block_8, 102);
        let winner_2 = submit_fork_block(&mut contract, &winner_1, 103);
        let winner_3 = submit_fork_block(&mut contract, &winner_2, 104);
        let winner_branch_tip = submit_fork_block(&mut contract, &winner_2, 105);

        // The fourth block outweighs the three main chain blocks above height 8
        let winner_tip = submit_fork_block(&mut contract, &winner_3, 106);

        assert_eq!(contract.mainchain_tip_blockhash, winner_tip);
        assert_eq!(
            contract.forks_tips,
            vec![
                ForkTip {
                    lca_height: 3,
                    tip_hash: untouched_tip.clone(),
                    tip_height: 4,
                    prefix_max_tip_height: 4,
                },
                // Branched off the old main chain at height 10, which is a fork now
                ForkTip {
                    lca_height: 8,
                    tip_hash: old_chain_branch_tip.clone(),
                    tip_height: 11,
                    prefix_max_tip_height: 11,
                },
                ForkTip {
                    lca_height: 8,
                    tip_hash: old_main_chain_tip.clone(),
                    tip_height: 11,
                    prefix_max_tip_height: 11,
                },
                // Branched off the winner, which is the main chain now
                ForkTip {
                    lca_height: 10,
                    tip_hash: winner_branch_tip.clone(),
                    tip_height: 11,
                    prefix_max_tip_height: 11,
                }
            ]
        );

        // With the LCA of the winner branch left at the reorg point, the GC would collect it
        // here as well
        contract.max_reorg = 2;
        contract.run_forks_gc(100);

        assert_eq!(
            contract.forks_tips,
            vec![ForkTip {
                lca_height: 10,
                tip_hash: winner_branch_tip,
                tip_height: 11,
                prefix_max_tip_height: 11,
            }]
        );
        assert!(!contract.headers_pool.contains_key(&old_main_chain_tip));
        assert!(!contract.headers_pool.contains_key(&old_chain_branch_tip));
        assert!(!contract.headers_pool.contains_key(&untouched_tip));
    }

    #[test]
    fn test_max_reorg_is_capped_by_the_gc_threshold() {
        let contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        assert_eq!(contract.gc_threshold, 3);
        assert_eq!(contract.get_max_reorg(), 3);
    }

    #[test]
    fn test_max_reorg_defaults_below_the_gc_threshold() {
        let contract = init_contract_with_chained_blocks();

        assert_eq!(contract.gc_threshold, DEFAULT_MAX_REORG);
        assert_eq!(contract.get_max_reorg(), DEFAULT_MAX_REORG);
    }

    #[test]
    fn test_get_forks_tips_skips_and_limits() {
        let mut contract = init_contract_with_chained_blocks();
        for (nonce, lca_height) in [(100, 8), (101, 3), (102, 5)] {
            let block_hash = contract.get_block_hash_by_height(lca_height).unwrap();
            submit_fork_block(&mut contract, &block_hash, nonce);
        }

        assert_eq!(contract.get_forks_tips(0, 10), contract.forks_tips);
        assert_eq!(
            contract.get_forks_tips(1, 1),
            contract.forks_tips[1..2].to_vec()
        );
        assert_eq!(
            contract.get_forks_tips(2, 10),
            contract.forks_tips[2..].to_vec()
        );
        assert!(contract.get_forks_tips(3, 10).is_empty());
    }

    #[test]
    fn test_submitting_new_fork_block_header() {
        let header = block_header_example();

        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        contract.submit_block_header(header.clone(), contract.skip_pow_verification);

        contract.submit_block_header(fork_block_header_example(), contract.skip_pow_verification);

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
                block_height: 1,
            }
        );
    }

    // test we can insert a block and get block back by it's height
    #[test]
    fn test_getting_block_by_height() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        contract.submit_block_header(block_header_example(), contract.skip_pow_verification);

        assert_eq!(
            contract.get_block_hash_by_height(0).unwrap(),
            genesis_block_header().block_hash(),
        );
        assert_eq!(
            contract.get_block_hash_by_height(1).unwrap(),
            block_header_example().block_hash()
        );
    }

    #[test]
    fn test_getting_height_by_block() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        contract.submit_block_header(block_header_example(), contract.skip_pow_verification);

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

        contract.submit_block_header(block_header_example(), contract.skip_pow_verification);

        contract.submit_block_header(fork_block_header_example(), contract.skip_pow_verification);
        contract.submit_block_header(
            fork_block_header_example_2(),
            contract.skip_pow_verification,
        );

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
                block_height: 2,
            }
        );
    }

    #[test]
    #[should_panic(expected = "bad-diffbits: incorrect proof of work")]
    fn test_submitting_block_with_incorrect_bits_same_period() {
        let mut contract = BtcLightClient::init(get_default_init_args());
        contract.bootstrap_end_height = 0;
        let mut next_header = block_header_example();
        next_header.bits += 1;
        contract.submit_block_header(next_header, contract.skip_pow_verification);
    }

    #[test]
    #[should_panic(expected = "PrevBlockNotFound")]
    fn test_getting_an_error_if_submitting_unattached_block() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());

        contract.submit_block_header(fork_block_header_example_2(), false);
    }

    fn init_real_genesis() -> BtcLightClient {
        let blocks = real_block_headers_685440_to_685451();
        BtcLightClient::init(InitArgs {
            network: Network::Mainnet,
            genesis_block_hash: blocks[0].block_hash(),
            genesis_block_height: 685440,
            skip_pow_verification: false,
            gc_threshold: 1000,
            genesis_block: blocks[0].clone(),
        })
    }

    #[test]
    fn test_bootstrap_end_height() {
        let blocks = real_block_headers_685440_to_685451();
        let mut contract = init_real_genesis();
        assert_eq!(contract.get_bootstrap_end_height(), 685452);

        for header in &blocks[1..11] {
            contract.submit_bootstrap_block_header(header.clone());
        }
        contract.finish_bootstrap_if_done();
        assert_eq!(contract.get_bootstrap_end_height(), 685452);

        contract.submit_bootstrap_block_header(blocks[11].clone());
        contract.finish_bootstrap_if_done();
        assert_eq!(contract.get_bootstrap_end_height(), 0);
    }

    #[test]
    fn test_no_bootstrap_with_skip_pow() {
        let contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        assert_eq!(contract.get_bootstrap_end_height(), 0);
    }

    #[test]
    #[should_panic(expected = "ERR_BOOTSTRAP_BLOCK_MUST_EXTEND_TIP")]
    fn test_bootstrap_block_must_extend_tip() {
        let blocks = real_block_headers_685440_to_685451();
        let mut contract = init_real_genesis();
        contract.submit_bootstrap_block_header(blocks[1].clone());
        contract.submit_bootstrap_block_header(blocks[1].clone());
    }

    #[test]
    #[should_panic(expected = "ERR_BLOCK_ABOVE_BOOTSTRAP_END")]
    fn test_bootstrap_block_above_end() {
        let blocks = real_block_headers_685440_to_685451();
        let mut contract = init_real_genesis();
        for header in &blocks[1..] {
            contract.submit_bootstrap_block_header(header.clone());
        }
        contract.submit_bootstrap_block_header(block_685452_header());
    }

    fn as_contract_account() {
        near_sdk::testing_env!(near_sdk::test_utils::VMContextBuilder::new()
            .predecessor_account_id(env::current_account_id())
            .attached_deposit(NearToken::from_near(1))
            .build());
    }

    #[test]
    fn test_bootstrap_blocks_by_contract_account() {
        let blocks = real_block_headers_685440_to_685451();
        let mut contract = init_real_genesis();
        as_contract_account();

        let _ = contract.bootstrap_blocks(blocks[1..].to_vec());
        assert_eq!(contract.get_bootstrap_end_height(), 0);
        assert_eq!(contract.get_last_block_height(), 685451);
    }

    #[test]
    #[should_panic(expected = "ERR_BOOTSTRAP_FINISHED")]
    fn test_bootstrap_blocks_after_finish() {
        let mut contract = BtcLightClient::init(get_default_init_args_with_skip_pow());
        as_contract_account();
        let _ = contract.bootstrap_blocks(vec![block_header_example()]);
    }

    #[test]
    #[should_panic(expected = "ERR_BOOTSTRAP_IN_PROGRESS")]
    fn test_submit_blocks_rejected_during_bootstrap() {
        let mut contract = init_real_genesis();
        let relayer: near_sdk::AccountId = "relayer.near".parse().unwrap();
        grant_role(&mut contract, Role::UnrestrictedSubmitBlocks, &relayer);
        let _ = contract.submit_blocks(real_block_headers_685440_to_685451()[1..2].to_vec());
    }

    #[test]
    fn test_migrate_from_v3_layout() {
        let contract = BtcLightClient::init(get_default_init_args());
        // V3: no `max_reorg`, `bootstrap_end_height` and `forks_tips`
        let raw_state = borsh::to_vec(&(
            &contract.mainchain_height_to_header,
            &contract.mainchain_header_to_height,
            &contract.mainchain_tip_blockhash,
            &contract.mainchain_initial_blockhash,
            &contract.headers_pool,
            contract.skip_pow_verification,
            contract.gc_threshold,
            &contract.network,
        ))
        .unwrap();
        env::storage_write(b"STATE", &raw_state);

        let migrated = BtcLightClient::migrate();
        assert_eq!(migrated.bootstrap_end_height, 0);
        assert_eq!(
            migrated.mainchain_tip_blockhash,
            contract.mainchain_tip_blockhash
        );
        assert!(matches!(migrated.network, Network::Mainnet));
        assert_eq!(migrated.gc_threshold, contract.gc_threshold);
        assert_eq!(
            migrated.max_reorg,
            std::cmp::min(DEFAULT_MAX_REORG, contract.gc_threshold)
        );
        assert!(migrated.forks_tips.is_empty());
    }

    #[test]
    fn test_migrate_from_v4_layout() {
        let contract = BtcLightClient::init(get_default_init_args());
        assert_ne!(contract.bootstrap_end_height, 0);
        // V4: no `max_reorg` and `forks_tips`
        let raw_state = borsh::to_vec(&(
            &contract.mainchain_height_to_header,
            &contract.mainchain_header_to_height,
            &contract.mainchain_tip_blockhash,
            &contract.mainchain_initial_blockhash,
            &contract.headers_pool,
            contract.skip_pow_verification,
            contract.gc_threshold,
            &contract.network,
            contract.bootstrap_end_height,
        ))
        .unwrap();
        env::storage_write(b"STATE", &raw_state);

        let migrated = BtcLightClient::migrate();
        assert_eq!(migrated.bootstrap_end_height, contract.bootstrap_end_height);
        assert_eq!(
            migrated.mainchain_tip_blockhash,
            contract.mainchain_tip_blockhash
        );
        assert!(matches!(migrated.network, Network::Mainnet));
        assert_eq!(migrated.gc_threshold, contract.gc_threshold);
        assert_eq!(
            migrated.max_reorg,
            std::cmp::min(DEFAULT_MAX_REORG, contract.gc_threshold)
        );
        assert!(migrated.forks_tips.is_empty());
    }
}
