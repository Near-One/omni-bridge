use near_sdk::near;

use crate::{hash::H256, header::Header, network::Network};

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct InitArgs {
    pub genesis_block_hash: H256,
    pub genesis_block_height: u64,
    pub skip_pow_verification: bool,
    pub gc_threshold: u64,
    pub network: Network,
    pub genesis_block: Header,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct ProofArgs {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub confirmations: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct ProofArgsV2 {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub coinbase_tx_id: H256,
    pub coinbase_merkle_proof: Vec<H256>,
    pub confirmations: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TxInclusionProof {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub coinbase_tx_id: H256,
    pub coinbase_merkle_proof: Vec<H256>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TxBlockMeta {
    pub target_block_height: u64,
    pub tip_block_height: u64,
    pub expected_merkle_root: H256,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TxInclusionInfo {
    pub tx_block_height: u64,
    pub mainchain_tip_height: u64,
}

impl From<ProofArgsV2> for ProofArgs {
    fn from(args: ProofArgsV2) -> Self {
        Self {
            tx_id: args.tx_id,
            tx_block_blockhash: args.tx_block_blockhash,
            tx_index: args.tx_index,
            merkle_proof: args.merkle_proof,
            confirmations: args.confirmations,
        }
    }
}

impl From<ProofArgsV2> for TxInclusionProof {
    fn from(args: ProofArgsV2) -> Self {
        Self {
            tx_id: args.tx_id,
            tx_block_blockhash: args.tx_block_blockhash,
            tx_index: args.tx_index,
            merkle_proof: args.merkle_proof,
            coinbase_tx_id: args.coinbase_tx_id,
            coinbase_merkle_proof: args.coinbase_merkle_proof,
        }
    }
}
