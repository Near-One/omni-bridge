use alloy::primitives::Address;
use near_primitives::types::AccountId;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub mainnet: Mainnet,
    pub testnet: Testnet,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Mainnet {
    pub eth_rpc_http_url: String,
    pub eth_rpc_ws_url: String,
    pub bridge_token_factory_address: Address,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Testnet {
    pub near_rpc_url: String,
    pub eth_rpc_url: String,
    pub eth_chain_id: u64,
    pub token_locker_id: AccountId,
    pub bridge_token_factory_address: Address,
    pub near_light_client_eth_address: Address,
}
