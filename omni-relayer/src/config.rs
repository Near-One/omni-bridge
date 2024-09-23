use alloy::primitives::Address;
use near_primitives::types::AccountId;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub near: Near,
    pub eth: Eth,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Near {
    pub rpc_url: String,
    pub token_locker_id: AccountId,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Eth {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub block_processing_batch_size: u64,
}
