use anyhow::{Context, Result};

use alloy::primitives::Address;
use near_primitives::{borsh::BorshDeserialize, types::AccountId};
use omni_types::{evm::utils::keccak256, OmniAddress, H160};

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
    pub relayer_address_on_evm: Option<OmniAddress>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Evm {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub block_processing_batch_size: u64,
}

impl Config {
    pub fn new(config_path: String) -> Result<Self> {
        let mut config = toml::from_str::<Self>(&std::fs::read_to_string(config_path)?)?;

        if config.near.relayer_address_on_evm.is_none() {
            let decoded_private_key = hex::decode(
                std::env::var("ETH_PRIVATE_KEY")
                    .context("Failed to get `ETH_PRIVATE_KEY` env variable")?,
            )
            .context("Failed to decode `ETH_PRIVATE_KEY`")?;
            let secret_key = secp256k1::SecretKey::from_slice(&decoded_private_key)
                .context("Failed to construct a `SecretKey` from given `ETH_PRIVATE_KEY`")?;

            let secp = secp256k1::Secp256k1::new();
            let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
            let public_key_bytes = public_key.serialize_uncompressed();
            let public_key_bytes = &public_key_bytes[1..];

            let result = keccak256(public_key_bytes);

            let address = &result[12..];

            config.near.relayer_address_on_evm = Some(OmniAddress::Eth(
                H160::try_from_slice(address)
                    .context("Failed to construct an `OmniAddress` from derived address")?,
            ));
        }

        Ok(config)
    }
}
