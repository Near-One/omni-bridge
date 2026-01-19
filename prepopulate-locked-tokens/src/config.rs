use anyhow::{Context, Result};
use near_api::AccountId;

#[derive(Debug, Clone)]
pub struct Config {
    pub omni_bridge_account_id: AccountId,
    pub near_rpc_url: String,
    pub solana_rpc_url: String,
    pub eth_rpc_url: String,
    pub arb_rpc_url: String,
    pub base_rpc_url: String,
    pub bnb_rpc_url: String,
    pub pol_rpc_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            omni_bridge_account_id: env_required("OMNI_BRIDGE_ACCOUNT_ID")?
                .parse()
                .context("Invalid OMNI_BRIDGE_ACCOUNT_ID")?,
            near_rpc_url: env_required("NEAR_RPC_URL")?,
            solana_rpc_url: env_required("SOLANA_RPC_URL")?,
            eth_rpc_url: env_required("ETH_RPC_URL")?,
            arb_rpc_url: env_required("ARB_RPC_URL")?,
            base_rpc_url: env_required("BASE_RPC_URL")?,
            bnb_rpc_url: env_required("BNB_RPC_URL")?,
            pol_rpc_url: env_required("POL_RPC_URL")?,
        })
    }
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).context(format!("Missing `{key}` env variable"))
}
