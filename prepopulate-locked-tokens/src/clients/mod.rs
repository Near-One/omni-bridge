use anyhow::Result;
use async_trait::async_trait;
use omni_types::{ChainKind, OmniAddress};
use std::sync::Arc;

use crate::config::Config;

pub mod evm;
pub mod near;
pub mod starknet;
pub mod svm;

/// A token representation's total supply together with the decimals of that
/// representation. Decimals differ per chain (EVM 18, Solana ~9, …), so we read them
/// from each representation rather than assuming a single token-wide value.
#[derive(Debug, Clone, Copy)]
pub struct TokenSupply {
    pub amount: u128,
    pub decimals: u8,
}

/// Reads the total supply (and decimals) of a token's representation on a single chain.
///
/// `token_address` may be the token on any chain; each client resolves the
/// representation on its own chain via the NEAR contract's `get_bridged_token`.
///
/// `Ok(None)` means the token has no representation on this chain (a clean skip,
/// distinct from `Err`, which is a genuine RPC/decode failure).
#[async_trait]
pub trait Client: Send + Sync {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<TokenSupply>>;

    /// The decimals of the token's representation on this chain. Reads decimals only
    /// (no supply, which can overflow `u128`) and returns the chain's native-coin
    /// decimals for a zero/native origin address. Used to read a token's origin decimals.
    async fn get_decimals(&self, token_address: OmniAddress) -> Result<Option<u8>>;
}

/// One client per supported destination chain.
pub struct Clients {
    pub near: Arc<near::Client>,
    pub eth: evm::Client,
    pub arb: evm::Client,
    pub base: evm::Client,
    pub bnb: evm::Client,
    pub pol: evm::Client,
    pub hlevm: evm::Client,
    pub abs: evm::Client,
    pub sol: svm::Client,
    pub fogo: svm::Client,
    pub strk: starknet::Client,
}

impl Clients {
    pub fn new(near_client: Arc<near::Client>, config: &Config) -> Result<Self> {
        let evm = |url: &str, chain| evm::Client::new(Arc::clone(&near_client), url.to_string(), chain);

        Ok(Self {
            near: Arc::clone(&near_client),
            eth: evm(&config.eth_rpc_url, ChainKind::Eth)?,
            arb: evm(&config.arb_rpc_url, ChainKind::Arb)?,
            base: evm(&config.base_rpc_url, ChainKind::Base)?,
            bnb: evm(&config.bnb_rpc_url, ChainKind::Bnb)?,
            pol: evm(&config.pol_rpc_url, ChainKind::Pol)?,
            hlevm: evm(&config.hlevm_rpc_url, ChainKind::HyperEvm)?,
            abs: evm(&config.abs_rpc_url, ChainKind::Abs)?,
            sol: svm::Client::new(
                Arc::clone(&near_client),
                config.solana_rpc_url.clone(),
                ChainKind::Sol,
            ),
            fogo: svm::Client::new(
                Arc::clone(&near_client),
                config.fogo_rpc_url.clone(),
                ChainKind::Fogo,
            ),
            strk: starknet::Client::new(Arc::clone(&near_client), &config.strk_rpc_url, ChainKind::Strk)?,
        })
    }

    /// The client that can read supply on `chain`, or `None` for chains with no
    /// queryable fungible representation (Btc/Zcash).
    pub fn client_for(&self, chain: ChainKind) -> Option<&dyn Client> {
        match chain {
            ChainKind::Near => Some(self.near.as_ref()),
            ChainKind::Eth => Some(&self.eth),
            ChainKind::Arb => Some(&self.arb),
            ChainKind::Base => Some(&self.base),
            ChainKind::Bnb => Some(&self.bnb),
            ChainKind::Pol => Some(&self.pol),
            ChainKind::HyperEvm => Some(&self.hlevm),
            ChainKind::Abs => Some(&self.abs),
            ChainKind::Sol => Some(&self.sol),
            ChainKind::Fogo => Some(&self.fogo),
            ChainKind::Strk => Some(&self.strk),
            ChainKind::Btc | ChainKind::Zcash => None,
        }
    }

    /// The concrete EVM client for an EVM chain (used by the solvency custody reader).
    pub fn evm_client(&self, chain: ChainKind) -> Option<&evm::Client> {
        match chain {
            ChainKind::Eth => Some(&self.eth),
            ChainKind::Arb => Some(&self.arb),
            ChainKind::Base => Some(&self.base),
            ChainKind::Bnb => Some(&self.bnb),
            ChainKind::Pol => Some(&self.pol),
            ChainKind::HyperEvm => Some(&self.hlevm),
            ChainKind::Abs => Some(&self.abs),
            _ => None,
        }
    }

    /// The concrete SVM client for an SVM chain (used by the solvency custody reader).
    pub fn svm_client(&self, chain: ChainKind) -> Option<&svm::Client> {
        match chain {
            ChainKind::Sol => Some(&self.sol),
            ChainKind::Fogo => Some(&self.fogo),
            _ => None,
        }
    }
}
