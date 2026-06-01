use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use near_api::{AccountId, Contract, Data, NetworkConfig, Signer, types::json::U128};
use omni_types::{ChainKind, OmniAddress};
use serde_json::json;
use std::sync::Arc;

use crate::apply::SetLockedTokenArg;

#[derive(Clone)]
pub struct Client {
    omni_bridge: Contract,
    network: NetworkConfig,
}

impl Client {
    pub fn new(omni_bridge_account_id: AccountId, rpc_url: &str) -> Result<Self> {
        let network =
            NetworkConfig::from_rpc_url("client", rpc_url.parse().context("Invalid NEAR RPC URL")?);

        Ok(Self {
            omni_bridge: Contract(omni_bridge_account_id),
            network,
        })
    }

    /// The token's representation on `chain`, or `None` if it has none there.
    ///
    /// The contract's `get_bridged_token` returns `Option<OmniAddress>` (JSON `null`
    /// when absent), so we decode into `Option` — a missing representation is a clean
    /// `None`, not an opaque deserialize error indistinguishable from an RPC failure.
    pub async fn get_bridged_token(
        &self,
        token_address: &OmniAddress,
        chain: ChainKind,
    ) -> Result<Option<OmniAddress>> {
        let result: Data<Option<OmniAddress>> = self
            .omni_bridge
            .call_function(
                "get_bridged_token",
                json!({
                    "address": token_address,
                    "chain": chain
                }),
            )
            .read_only()
            .fetch_from(&self.network)
            .await
            .with_context(|| {
                format!("Failed to fetch bridged token ({token_address}) on {chain:?}")
            })?;

        Ok(result.data)
    }

    /// Current on-chain locked amount for `(chain_kind, token_id)`, if any.
    pub async fn get_locked_tokens(
        &self,
        chain_kind: ChainKind,
        token_id: &AccountId,
    ) -> Result<Option<u128>> {
        let result: Data<Option<U128>> = self
            .omni_bridge
            .call_function(
                "get_locked_tokens",
                json!({
                    "chain_kind": chain_kind,
                    "token_id": token_id,
                }),
            )
            .read_only()
            .fetch_from(&self.network)
            .await
            .with_context(|| {
                format!("Failed to fetch locked tokens for ({chain_kind:?}, {token_id})")
            })?;

        Ok(result.data.map(|amount| amount.0))
    }

    /// NEP-141 `ft_balance_of(account_id)` on `token_contract` (used to read the
    /// bridge's custody balance of a NEAR-origin token).
    pub async fn ft_balance_of(
        &self,
        token_contract: &AccountId,
        account_id: &AccountId,
    ) -> Result<u128> {
        let balance: Data<U128> = Contract(token_contract.clone())
            .call_function("ft_balance_of", json!({ "account_id": account_id }))
            .read_only()
            .fetch_from(&self.network)
            .await
            .with_context(|| format!("Failed to fetch ft_balance_of({account_id}) on {token_contract}"))?;

        Ok(balance.data.0)
    }

    /// Send a single `set_locked_tokens` transaction with the given batch of args.
    /// Returns an error (without panicking) if the transaction executes with failure.
    pub async fn set_locked_tokens(
        &self,
        signer_id: AccountId,
        signer: Arc<Signer>,
        args: &[SetLockedTokenArg],
    ) -> Result<()> {
        let outcome = self
            .omni_bridge
            .call_function("set_locked_tokens", json!({ "args": args }))
            .transaction()
            .max_gas()
            .with_signer(signer_id, signer)
            .send_to(&self.network)
            .await
            .context("Failed to send set_locked_tokens transaction")?;

        outcome
            .into_result()
            .map_err(|err| anyhow!("set_locked_tokens transaction failed: {err:?}"))?;

        Ok(())
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<u128>> {
        let token_id = match token_address {
            OmniAddress::Near(token_id) => token_id,
            address => match self.get_bridged_token(&address, ChainKind::Near).await? {
                Some(OmniAddress::Near(token_id)) => token_id,
                Some(other) => bail!("Unexpected address type ({other}) for Near chain"),
                None => return Ok(None),
            },
        };

        let total_supply: Data<U128> = Contract(token_id)
            .call_function("ft_total_supply", ())
            .read_only()
            .fetch_from(&self.network)
            .await?;

        Ok(Some(total_supply.data.0))
    }
}
