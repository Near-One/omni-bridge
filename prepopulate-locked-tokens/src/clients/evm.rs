use anyhow::{Context, Result};
use async_trait::async_trait;
use omni_types::{ChainKind, H160, OmniAddress};
use std::sync::Arc;

use alloy::{
    primitives::Address,
    providers::{DynProvider, Provider, ProviderBuilder},
    sol,
};

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function totalSupply() external view returns (uint256 totalSupply);
        function balanceOf(address account) external view returns (uint256 balance);
        function decimals() external view returns (uint8 decimals);
    }
}

/// Native coins (origin address `0x0`) have no ERC-20 to query; EVM native is 18 decimals.
const NATIVE_DECIMALS: u8 = 18;

pub struct Client {
    near_client: Arc<super::near::Client>,
    provider: DynProvider,
    chain: ChainKind,
}

impl Client {
    pub fn new(
        near_client: Arc<super::near::Client>,
        rpc_http_url: String,
        chain: ChainKind,
    ) -> Result<Self> {
        if !chain.is_evm_chain() {
            anyhow::bail!("Unsupported chain kind: {chain:?}");
        }

        let rpc_http_url = rpc_http_url
            .parse()
            .context(format!("Failed to parse {chain:?} RPC HTTP URL"))?;
        let provider = DynProvider::new(ProviderBuilder::new().connect_http(rpc_http_url));

        Ok(Self {
            near_client,
            provider,
            chain,
        })
    }

    /// Resolve the token's address on this EVM chain, or `None` if not deployed here.
    async fn resolve_address(&self, token_address: OmniAddress) -> Result<Option<H160>> {
        let address = match token_address {
            OmniAddress::Eth(address) if self.chain == ChainKind::Eth => address,
            OmniAddress::Arb(address) if self.chain == ChainKind::Arb => address,
            OmniAddress::Base(address) if self.chain == ChainKind::Base => address,
            OmniAddress::Bnb(address) if self.chain == ChainKind::Bnb => address,
            OmniAddress::Pol(address) if self.chain == ChainKind::Pol => address,
            OmniAddress::HyperEvm(address) if self.chain == ChainKind::HyperEvm => address,
            OmniAddress::Abs(address) if self.chain == ChainKind::Abs => address,
            address => match self.near_client.get_bridged_token(&address, self.chain).await? {
                Some(bridged) => self.match_evm_address(bridged)?,
                None => return Ok(None),
            },
        };
        Ok(Some(address))
    }

    fn match_evm_address(&self, address: OmniAddress) -> Result<H160> {
        match (self.chain, address) {
            (ChainKind::Eth, OmniAddress::Eth(address)) => Ok(address),
            (ChainKind::Arb, OmniAddress::Arb(address)) => Ok(address),
            (ChainKind::Base, OmniAddress::Base(address)) => Ok(address),
            (ChainKind::Bnb, OmniAddress::Bnb(address)) => Ok(address),
            (ChainKind::Pol, OmniAddress::Pol(address)) => Ok(address),
            (ChainKind::HyperEvm, OmniAddress::HyperEvm(address)) => Ok(address),
            (ChainKind::Abs, OmniAddress::Abs(address)) => Ok(address),
            (chain, address) => {
                anyhow::bail!("Unexpected address type ({address}) for {chain:?} chain")
            }
        }
    }

    /// ERC-20 `balanceOf(holder)` for `token` (the bridge's custody balance of an
    /// EVM-origin token).
    pub async fn balance_of(&self, token: H160, holder: H160) -> Result<u128> {
        let erc20 = IERC20::new(Address::from_slice(&token.0), &self.provider);
        let balance = erc20
            .balanceOf(Address::from_slice(&holder.0))
            .call()
            .await
            .with_context(|| format!("Failed to fetch balanceOf({holder}) for {token} on {:?}", self.chain))?;
        balance
            .try_into()
            .with_context(|| format!("balanceOf({holder}) exceeds u128 on {:?}", self.chain))
    }

    /// Whether `address` has contract code on this chain. Used by the solvency reader: a
    /// token contract that doesn't exist can hold no balance, so a non-contract origin
    /// address means custody 0 rather than a read failure (`balanceOf` reverts with `0x`
    /// on an address with no code).
    pub async fn is_contract(&self, address: H160) -> Result<bool> {
        let code = self
            .provider
            .get_code_at(Address::from_slice(&address.0))
            .await
            .with_context(|| format!("Failed to fetch code at {address} on {:?}", self.chain))?;
        Ok(!code.is_empty())
    }

    /// Native coin balance of `holder` (for a native EVM origin, where the token's
    /// origin address is the zero address).
    pub async fn native_balance(&self, holder: H160) -> Result<u128> {
        let balance = self
            .provider
            .get_balance(Address::from_slice(&holder.0))
            .await
            .with_context(|| format!("Failed to fetch native balance of {holder} on {:?}", self.chain))?;
        balance
            .try_into()
            .with_context(|| format!("native balance of {holder} exceeds u128 on {:?}", self.chain))
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<super::TokenSupply>> {
        let Some(token_address) = self.resolve_address(token_address).await? else {
            return Ok(None);
        };

        let address = Address::from_slice(&token_address.0);
        let erc20 = IERC20::new(address, &self.provider);
        let total_supply = erc20.totalSupply().call().await.context(format!(
            "Failed to fetch total supply for token {token_address} on {:?}",
            self.chain
        ))?;
        let amount = total_supply.try_into().context(format!(
            "Failed to parse total supply for token {token_address} on {:?}",
            self.chain
        ))?;
        let decimals = erc20.decimals().call().await.context(format!(
            "Failed to fetch decimals for token {token_address} on {:?}",
            self.chain
        ))?;

        Ok(Some(super::TokenSupply { amount, decimals }))
    }

    async fn get_decimals(&self, token_address: OmniAddress) -> Result<Option<u8>> {
        let Some(token_address) = self.resolve_address(token_address).await? else {
            return Ok(None);
        };

        // Native coin (zero address) has no ERC-20 to query.
        if token_address.0 == [0u8; 20] {
            return Ok(Some(NATIVE_DECIMALS));
        }

        let erc20 = IERC20::new(Address::from_slice(&token_address.0), &self.provider);
        let decimals = erc20.decimals().call().await.context(format!(
            "Failed to fetch decimals for token {token_address} on {:?}",
            self.chain
        ))?;
        Ok(Some(decimals))
    }
}
