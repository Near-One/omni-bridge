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
    pub chain_id: u64,
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub bridge_token_factory_address: Address,
}
