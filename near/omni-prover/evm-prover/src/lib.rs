use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, ext_contract, near_bindgen, require, AccountId, Gas, PanicOnDefault, Promise};
use omni_types::evm::events::parse_evm_event;
use omni_types::evm::header::BlockHeader;
use omni_types::evm::receipt::{LogEntry, Receipt};
use omni_types::evm::utils::keccak256;
use omni_types::prover_args::EvmVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};
use omni_types::ChainKind;
use rlp::Rlp;

const VERIFY_PROOF_CALLBACK_GAS: Gas = Gas::from_tgas(20);
const BLOCK_HASH_SAFE_GAS: Gas = Gas::from_tgas(10);

type H256 = [u8; 32];

/// Defines an interface to call EthClient contract to get the safe block hash for a given block
/// number. It returns Some(hash) if the block hash is present in the safe canonical chain, or
/// None if the block number is not part of the canonical chain yet.
#[ext_contract(evm_client)]
pub trait EvmClient {
    #[result_serializer(borsh)]
    fn block_hash_safe(&self, #[serializer(borsh)] index: u64) -> Option<H256>;
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct EvmProver {
    pub light_client: AccountId,
    pub chain_kind: ChainKind,
}

#[near_bindgen]
impl EvmProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(light_client: AccountId, chain_kind: ChainKind) -> Self {
        Self {
            light_client,
            chain_kind,
        }
    }

    /// # Panics
    ///
    /// This function will panic in the following situations:
    /// - If the log entry at the specified index doesn't match the decoded log entry.
    #[allow(clippy::needless_pass_by_value)]
    #[handle_result]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Result<Promise, String> {
        let args = EvmVerifyProofArgs::try_from_slice(&input).map_err(|_| "ERR_PARSE_ARGS")?;

        let evm_proof = args.proof;
        let header: BlockHeader = rlp::decode(&evm_proof.header_data).map_err(|e| e.to_string())?;
        let log_entry: LogEntry =
            rlp::decode(&evm_proof.log_entry_data).map_err(|e| e.to_string())?;
        let receipt: Receipt = rlp::decode(&evm_proof.receipt_data).map_err(|e| e.to_string())?;

        // Verify log_entry included in receipt
        let log_index_usize = usize::try_from(evm_proof.log_index).map_err(|e| e.to_string())?;
        require!(receipt.logs[log_index_usize] == log_entry);

        // Verify receipt included into header
        let data = Self::verify_trie_proof(
            header.receipts_root.0,
            rlp::encode(&evm_proof.receipt_index).to_vec(),
            &evm_proof.proof,
        );

        if evm_proof.receipt_data != data {
            return Err("ERR_INVALID_PROOF".to_owned());
        }

        // Verify block header was in the bridge
        Ok(evm_client::ext(self.light_client.clone())
            .with_static_gas(BLOCK_HASH_SAFE_GAS)
            .block_hash_safe(header.number.as_u64())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_PROOF_CALLBACK_GAS)
                    .verify_proof_callback(
                        args.proof_kind,
                        evm_proof.log_entry_data,
                        header.hash.ok_or("ERR_HASH_NOT_SET")?.0,
                    ),
            ))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[private]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof_callback(
        &mut self,
        #[serializer(borsh)] kind: ProofKind,
        #[serializer(borsh)] log_entry_data: Vec<u8>,
        #[serializer(borsh)] expected_block_hash: H256,
        #[callback]
        #[serializer(borsh)]
        block_hash: Option<H256>,
    ) -> Result<ProverResult, String> {
        if block_hash != Some(expected_block_hash) {
            return Err("ERR_INVALID_BLOCK_HASH".to_owned());
        }

        match kind {
            ProofKind::InitTransfer => Ok(ProverResult::InitTransfer(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
            ProofKind::FinTransfer => Ok(ProverResult::FinTransfer(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
            ProofKind::DeployToken => Ok(ProverResult::DeployToken(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
            ProofKind::LogMetadata => Ok(ProverResult::LogMetadata(parse_evm_event(
                self.chain_kind,
                log_entry_data,
            )?)),
        }
    }

    /// Verify the proof recursively traversing through the key.
    /// Return the value at the end of the key, in case the proof is valid.
    ///
    /// @param expected_root is the expected root of the current node.
    /// @param key is the key for which we are proving the value.
    /// @param proof contains relevant information to verify data is valid
    ///
    /// Patricia Trie: https://eth.wiki/en/fundamentals/patricia-tree
    /// Patricia Img:  https://ethereum.stackexchange.com/questions/268/ethereum-block-architecture/6413#6413
    ///
    /// Verification:  https://github.com/slockit/in3/wiki/Ethereum-Verification-and-MerkleProof#receipt-proof
    /// Article:       https://medium.com/@ouvrard.pierre.alain/merkle-proof-verification-for-ethereum-patricia-tree-48f29658eec
    /// Python impl:   https://gist.github.com/mfornet/0ff283274c0162f1cca45966bccf69ee
    ///
    fn verify_trie_proof(expected_root: H256, key: Vec<u8>, proof: &Vec<Vec<u8>>) -> Vec<u8> {
        let mut actual_key = vec![];
        for el in key {
            actual_key.push(el / 16);
            actual_key.push(el % 16);
        }
        Self::_verify_trie_proof(expected_root.to_vec(), &actual_key, proof, 0, 0)
    }

    #[allow(clippy::needless_pass_by_value)]
    fn _verify_trie_proof(
        expected_root: Vec<u8>,
        key: &Vec<u8>,
        proof: &Vec<Vec<u8>>,
        key_index: usize,
        proof_index: usize,
    ) -> Vec<u8> {
        let node = &proof[proof_index];

        if key_index == 0 {
            // trie root is always a hash
            require!(keccak256(node) == expected_root.as_slice());
        } else if node.len() < 32 {
            // if rlp < 32 bytes, then it is not hashed
            require!(node.as_slice() == expected_root);
        } else {
            require!(keccak256(node) == expected_root.as_slice());
        }

        let node = Rlp::new(node.as_slice());

        if node.iter().count() == 17 {
            // Branch node
            if key_index >= key.len() {
                require!(proof_index + 1 == proof.len());
                get_vec(&node, 16)
            } else {
                let new_expected_root = get_vec(&node, key[key_index] as usize);
                if new_expected_root.is_empty() {
                    // not included in proof
                    vec![]
                } else {
                    Self::_verify_trie_proof(
                        new_expected_root,
                        key,
                        proof,
                        key_index + 1,
                        proof_index + 1,
                    )
                }
            }
        } else {
            // Leaf or extension node
            require!(node.iter().count() == 2);
            let path_u8 = get_vec(&node, 0);
            // Extract first nibble
            let head = path_u8[0] / 16;
            // require!(0 <= head); is implicit because of type limits
            require!(head <= 3);

            // Extract path
            let mut path = vec![];
            if head % 2 == 1 {
                path.push(path_u8[0] % 16);
            }
            for val in path_u8.iter().skip(1) {
                path.push(val / 16);
                path.push(val % 16);
            }

            if head >= 2 {
                // Leaf node
                require!(proof_index + 1 == proof.len());
                require!(key_index + path.len() == key.len());
                if path.as_slice() == &key[key_index..key_index + path.len()] {
                    get_vec(&node, 1)
                } else {
                    vec![]
                }
            } else {
                // Extension node
                require!(path.as_slice() == &key[key_index..key_index + path.len()]);
                let new_expected_root = get_vec(&node, 1);
                Self::_verify_trie_proof(
                    new_expected_root,
                    key,
                    proof,
                    key_index + path.len(),
                    proof_index + 1,
                )
            }
        }
    }
}

/// Get element at position `pos` from rlp encoded data,
/// and decode it as vector of bytes
fn get_vec(data: &Rlp, pos: usize) -> Vec<u8> {
    data.at(pos).unwrap().as_val::<Vec<u8>>().unwrap()
}
