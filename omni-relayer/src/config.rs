use alloy::primitives::Address;
use near_primitives::types::AccountId;
use omni_types::ChainKind;

pub fn get_private_key(chain_kind: ChainKind) -> String {
    let env_var = match chain_kind {
        ChainKind::Near => "NEAR_PRIVATE_KEY",
        ChainKind::Eth => "ETH_PRIVATE_KEY",
        ChainKind::Base => "BASE_PRIVATE_KEY",
        ChainKind::Arb => "ARB_PRIVATE_KEY",
        ChainKind::Sol => "SOLANA_PRIVATE_KEY",
    };

    std::env::var(env_var).unwrap_or_else(|_| panic!("Failed to get `{env_var}` env variable"))
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub near: Near,
    pub eth: Option<Evm>,
    pub base: Option<Evm>,
    pub arb: Option<Evm>,
    pub solana: Option<Solana>,
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
pub struct Solana {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub program_id: String,
    pub wormhole_id: String,
    pub init_transfer_token_index: usize,
    pub init_transfer_emitter_index: usize,
    pub init_transfer_sol_emitter_index: usize,
    pub init_transfer_discriminator: Vec<u8>,
    pub init_transfer_sol_discriminator: Vec<u8>,
    pub finalize_transfer_emitter_index: usize,
    pub finalize_transfer_sol_emitter_index: usize,
    pub finalize_transfer_discriminator: Vec<u8>,
    pub finalize_transfer_sol_discriminator: Vec<u8>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Wormhole {
    pub api_url: String,
    pub eth_chain_id: u64,
    pub base_chain_id: u64,
    pub arb_chain_id: u64,
    pub solana_chain_id: u64,
}
