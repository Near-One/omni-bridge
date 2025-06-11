use alloy::{
    primitives::Address,
    signers::{k256::ecdsa::SigningKey, local::LocalSigner},
};
use near_primitives::types::AccountId;
use omni_types::{ChainKind, OmniAddress};
use serde::Deserialize;

pub enum NearSignerType {
    Omni,
    Fast,
}

pub fn get_private_key(chain_kind: ChainKind, near_signer_type: Option<NearSignerType>) -> String {
    let env_var = match chain_kind {
        ChainKind::Near => match near_signer_type.unwrap() {
            NearSignerType::Omni => "NEAR_OMNI_PRIVATE_KEY",
            NearSignerType::Fast => "NEAR_FAST_PRIVATE_KEY",
        },
        ChainKind::Eth => "ETH_PRIVATE_KEY",
        ChainKind::Base => "BASE_PRIVATE_KEY",
        ChainKind::Arb => "ARB_PRIVATE_KEY",
        ChainKind::Sol => "SOLANA_PRIVATE_KEY",
    };

    std::env::var(env_var).unwrap_or_else(|_| panic!("Failed to get `{env_var}` env variable"))
}

pub fn get_relayer_evm_address(chain_kind: ChainKind) -> Address {
    let decoded_private_key =
        hex::decode(get_private_key(chain_kind, None)).expect("Failed to decode EVM private key");

    let secret_key = SigningKey::from_slice(&decoded_private_key)
        .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    signer.address()
}

fn replace_mongodb_credentials<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let uri = Option::<String>::deserialize(deserializer)?;

    if let Some(uri) = uri {
        let username = std::env::var("MONGODB_USERNAME").map_err(serde::de::Error::custom)?;
        let password = std::env::var("MONGODB_PASSWORD").map_err(serde::de::Error::custom)?;
        let host = std::env::var("MONGODB_HOST").map_err(serde::de::Error::custom)?;

        Ok(Some(
            uri.replace("MONGODB_USERNAME", &username)
                .replace("MONGODB_PASSWORD", &password)
                .replace("MONGODB_HOST", &host),
        ))
    } else {
        Ok(None)
    }
}

fn replace_rpc_api_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let url = String::deserialize(deserializer)?;

    let api_key = std::env::var("INFURA_API_KEY").map_err(serde::de::Error::custom)?;

    Ok(url.replace("INFURA_API_KEY", &api_key))
}

fn validate_fee_discount<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let fee_discount = u8::deserialize(deserializer)?;

    if fee_discount > 100 {
        return Err(serde::de::Error::custom(
            "Fee discount should be less than 100",
        ));
    }

    Ok(fee_discount)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub bridge_indexer: BridgeIndexer,
    pub near: Near,
    pub eth: Option<Evm>,
    pub base: Option<Evm>,
    pub arb: Option<Evm>,
    pub solana: Option<Solana>,
    pub btc: Option<Btc>,
    pub wormhole: Wormhole,
}

impl Config {
    pub const fn is_bridge_indexer_enabled(&self) -> bool {
        self.bridge_indexer.mongodb_uri.is_some() && self.bridge_indexer.db_name.is_some()
    }

    pub const fn is_bridge_api_enabled(&self) -> bool {
        self.bridge_indexer.api_url.is_some()
    }

    pub fn is_fast_relayer_enabled(&self) -> bool {
        self.near.fast_relayer_enabled.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeIndexer {
    pub api_url: Option<String>,

    #[serde(default, deserialize_with = "replace_mongodb_credentials")]
    pub mongodb_uri: Option<String>,
    pub db_name: Option<String>,

    #[serde(default, deserialize_with = "validate_fee_discount")]
    pub fee_discount: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Testnet,
    Mainnet,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Near {
    pub network: Network,
    pub rpc_url: String,
    pub omni_bridge_id: AccountId,
    pub btc_connector: Option<AccountId>,
    pub btc: Option<AccountId>,
    pub satoshi_relayer: Option<AccountId>,
    pub omni_credentials_path: Option<String>,
    pub fast_credentials_path: Option<String>,
    pub sign_without_checking_fee: Option<Vec<OmniAddress>>,
    pub fast_relayer_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Evm {
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_http_url: String,
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub omni_bridge_address: Address,
    pub light_client: Option<AccountId>,
    pub block_processing_batch_size: u64,
    pub expected_finalization_time: i64,
    pub safe_confirmations: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Solana {
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_http_url: String,
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_ws_url: String,
    pub program_id: String,
    pub wormhole_id: String,
    pub deploy_token_emitter_index: usize,
    pub deploy_token_discriminator: Vec<u8>,
    pub init_transfer_sender_index: usize,
    pub init_transfer_token_index: usize,
    pub init_transfer_emitter_index: usize,
    pub init_transfer_sol_sender_index: usize,
    pub init_transfer_sol_emitter_index: usize,
    pub init_transfer_discriminator: Vec<u8>,
    pub init_transfer_sol_discriminator: Vec<u8>,
    pub finalize_transfer_emitter_index: usize,
    pub finalize_transfer_sol_emitter_index: usize,
    pub finalize_transfer_discriminator: Vec<u8>,
    pub finalize_transfer_sol_discriminator: Vec<u8>,
    pub credentials_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Btc {
    pub rpc_http_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Wormhole {
    pub api_url: String,
    pub solana_chain_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FastTransfers {}
