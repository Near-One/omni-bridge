use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use omni_types::{ChainKind, OmniAddress};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

/// Native SOL/Fogo (origin = system program, all-zero mint) is 9 decimals.
const NATIVE_DECIMALS: u8 = 9;

/// SPL-token supply / balance reader for SVM chains (Solana and Fogo).
pub struct Client {
    near_client: Arc<super::near::Client>,
    rpc_client: RpcClient,
    chain: ChainKind,
}

impl Client {
    pub fn new(near_client: Arc<super::near::Client>, rpc_http_url: String, chain: ChainKind) -> Self {
        Self {
            near_client,
            rpc_client: RpcClient::new(rpc_http_url),
            chain,
        }
    }

    /// Resolve the 32-byte SVM mint for this chain, or `None` if not deployed here.
    async fn resolve_mint(&self, token_address: OmniAddress) -> Result<Option<[u8; 32]>> {
        let mint = match token_address {
            OmniAddress::Sol(addr) if self.chain == ChainKind::Sol => addr.0,
            OmniAddress::Fogo(addr) if self.chain == ChainKind::Fogo => addr.0,
            address => match self.near_client.get_bridged_token(&address, self.chain).await? {
                Some(bridged) => self.match_svm_mint(bridged)?,
                None => return Ok(None),
            },
        };
        Ok(Some(mint))
    }

    fn match_svm_mint(&self, address: OmniAddress) -> Result<[u8; 32]> {
        match (self.chain, address) {
            (ChainKind::Sol, OmniAddress::Sol(addr)) => Ok(addr.0),
            (ChainKind::Fogo, OmniAddress::Fogo(addr)) => Ok(addr.0),
            (chain, address) => {
                bail!("Unexpected address type ({address}) for {chain:?} chain")
            }
        }
    }

    /// Raw balance of an SPL token account (the bridge's custody vault).
    pub async fn token_account_balance(&self, account: &Pubkey) -> Result<u128> {
        let balance = self
            .rpc_client
            .get_token_account_balance(account)
            .await
            .with_context(|| format!("Failed to get token account balance on {:?}", self.chain))?;
        balance
            .amount
            .parse::<u128>()
            .with_context(|| format!("Failed to parse token account balance on {:?}", self.chain))
    }

    /// Native lamports held by an account (the native-SOL custody vault).
    pub async fn account_lamports(&self, account: &Pubkey) -> Result<u128> {
        let lamports = self
            .rpc_client
            .get_balance(account)
            .await
            .with_context(|| format!("Failed to get lamports on {:?}", self.chain))?;
        Ok(u128::from(lamports))
    }
}

/// Token-vault PDA holding an SVM-origin token's locked balance:
/// `find_program_address(&[b"vault", mint], program)`.
pub fn derive_token_vault(program: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", mint.as_ref()], program).0
}

/// Native-SOL vault PDA holding locked lamports: `find_program_address(&[b"sol_vault"], program)`.
pub fn derive_sol_vault(program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"sol_vault"], program).0
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<super::TokenSupply>> {
        let Some(mint_bytes) = self.resolve_mint(token_address).await? else {
            return Ok(None);
        };

        let mint = Pubkey::new_from_array(mint_bytes);
        let supply = self
            .rpc_client
            .get_token_supply(&mint)
            .await
            .with_context(|| format!("Failed to get token supply on {:?}", self.chain))?;
        let amount = supply
            .amount
            .parse::<u128>()
            .with_context(|| format!("Failed to parse total supply on {:?}", self.chain))?;

        // The mint's own decimals come back with the supply — no extra call needed.
        Ok(Some(super::TokenSupply {
            amount,
            decimals: supply.decimals,
        }))
    }

    async fn get_decimals(&self, token_address: OmniAddress) -> Result<Option<u8>> {
        let Some(mint_bytes) = self.resolve_mint(token_address).await? else {
            return Ok(None);
        };

        // Native SOL/Fogo (all-zero mint = system program) has no SPL mint to query.
        if mint_bytes == [0u8; 32] {
            return Ok(Some(NATIVE_DECIMALS));
        }

        let mint = Pubkey::new_from_array(mint_bytes);
        let supply = self
            .rpc_client
            .get_token_supply(&mint)
            .await
            .with_context(|| format!("Failed to get mint decimals on {:?}", self.chain))?;
        Ok(Some(supply.decimals))
    }
}
