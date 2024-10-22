use alloy::primitives::Address;
use alloy::signers::k256::ecdsa::SigningKey;
use alloy::signers::local::LocalSigner;
use near_primitives::{borsh::BorshDeserialize, types::AccountId};
use omni_types::{OmniAddress, H160};

fn derive_evm_address_from_private_key() -> OmniAddress {
    let decoded_private_key = hex::decode(
        std::env::var("ETH_PRIVATE_KEY").expect("Failed to get `ETH_PRIVATE_KEY` env variable"),
    )
    .expect("Failed to decode `ETH_PRIVATE_KEY`");

    let secret_key = SigningKey::from_slice(&decoded_private_key)
        .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    OmniAddress::Eth(
        H160::try_from_slice(signer.address().as_slice())
            .expect("Failed to create `OmniAddress` from the derived public key"),
    )
}

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
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Evm {
    pub rpc_http_url: String,
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub block_processing_batch_size: u64,

    #[serde(default = "derive_evm_address_from_private_key")]
    pub relayer_address_on_eth: OmniAddress,

    pub init_transfer_gas_estimation: u128,
    pub fin_transfer_gas_estimation: u128,
}
