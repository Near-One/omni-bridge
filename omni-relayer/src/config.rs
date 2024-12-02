use alloy::primitives::Address;
use near_primitives::types::AccountId;
use omni_types::ChainKind;

pub fn get_evm_private_key(chain_kind: ChainKind) -> String {
    let env_var = match chain_kind {
        ChainKind::Eth => "ETH_PRIVATE_KEY",
        ChainKind::Base => "BASE_PRIVATE_KEY",
        ChainKind::Arb => "ARB_PRIVATE_KEY",
        _ => unreachable!("Unsupported chain kind"),
    };

    std::env::var(env_var).unwrap_or_else(|_| panic!("Failed to get `{env_var}` env variable"))
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub near: Near,
    pub eth: Evm,
    pub base: Evm,
    pub arb: Evm,
    pub wormhole: Wormhole,
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
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Evm {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub light_client: Option<AccountId>,
    pub block_processing_batch_size: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Wormhole {
    pub api_url: String,
    pub eth_chain_id: u64,
    pub base_chain_id: u64,
    pub arb_chain_id: u64,
}
