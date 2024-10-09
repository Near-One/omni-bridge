use alloy::primitives::Address;
use near_primitives::types::AccountId;
use omni_types::OmniAddress;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub near: Near,
    pub evm: Evm,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Testnet,
    Mainnet,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Near {
    pub network: Network,
    pub rpc_url: String,
    pub token_locker_id: AccountId,
    pub credentials_path: Option<String>,
    pub eth_light_client: AccountId,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Evm {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub block_processing_batch_size: u64,
    pub relayer: OmniAddress,
}
