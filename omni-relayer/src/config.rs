use alloy::primitives::Address;
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use near_primitives::{borsh::BorshDeserialize, types::AccountId};
use omni_types::{ChainKind, OmniAddress, H160};

pub fn get_evm_private_key(chain_kind: ChainKind) -> String {
    let env_var = match chain_kind {
        ChainKind::Eth => "ETH_PRIVATE_KEY",
        ChainKind::Base => "BASE_PRIVATE_KEY",
        ChainKind::Arb => "ARB_PRIVATE_KEY",
        _ => unreachable!("Unsupported chain kind"),
    };

    std::env::var(env_var).unwrap_or_else(|_| panic!("Failed to get `{env_var}` env variable"))
}

fn derive_eth_address_from_private_key() -> OmniAddress {
    let private_key = get_evm_private_key(ChainKind::Eth);

    let secret_key = SigningKey::from_slice(
        &hex::decode(private_key).expect("Failed to decode `ETH_PRIVATE_KEY` env variable"),
    )
    .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    OmniAddress::Eth(
        H160::try_from_slice(signer.address().as_slice())
            .expect("Failed to create `OmniAddress` from the derived public key"),
    )
}

fn derive_base_address_from_private_key() -> OmniAddress {
    let private_key = get_evm_private_key(ChainKind::Base);

    let secret_key = SigningKey::from_slice(
        &hex::decode(private_key).expect("Failed to decode `BASE_PRIVATE_KEY` env variable"),
    )
    .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    OmniAddress::Base(
        H160::try_from_slice(signer.address().as_slice())
            .expect("Failed to create `OmniAddress` from the derived public key"),
    )
}

fn derive_arb_address_from_private_key() -> OmniAddress {
    let private_key = get_evm_private_key(ChainKind::Arb);

    let secret_key = SigningKey::from_slice(
        &hex::decode(private_key).expect("Failed to decode `ARB_PRIVATE_KEY` env variable"),
    )
    .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    OmniAddress::Arb(
        H160::try_from_slice(signer.address().as_slice())
            .expect("Failed to create `OmniAddress` from the derived public key"),
    )
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub near: Near,
    pub eth: Eth,
    pub base: Base,
    pub arb: Arb,
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
pub struct Eth {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub light_client: AccountId,
    #[serde(default = "derive_eth_address_from_private_key")]
    pub relayer_address: OmniAddress,

    pub block_processing_batch_size: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Base {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    #[serde(default = "derive_base_address_from_private_key")]
    pub relayer_address: OmniAddress,

    pub block_processing_batch_size: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Arb {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    #[serde(default = "derive_arb_address_from_private_key")]
    pub relayer_address: OmniAddress,

    pub block_processing_batch_size: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Wormhole {
    pub api_url: String,

    pub eth_chain_id: u64,
    pub base_chain_id: u64,
    pub arb_chain_id: u64,
}
