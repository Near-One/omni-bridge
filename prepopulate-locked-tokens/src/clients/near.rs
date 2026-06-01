use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use near_api::{AccountId, Contract, Data, NetworkConfig, RPCEndpoint, Signer, types::json::U128};
use omni_types::{ChainKind, OmniAddress};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

use crate::apply::SetLockedTokenArg;

/// View calls are retried on transient transport errors. near_api classifies connection
/// errors (`CommunicationError`) as "critical" and does NOT retry them itself, but under
/// concurrent load they're usually just a dropped connection.
const MAX_READ_ATTEMPTS: u32 = 4;

#[derive(Clone)]
pub struct Client {
    omni_bridge: Contract,
    network: NetworkConfig,
}

impl Client {
    pub fn new(omni_bridge_account_id: AccountId, rpc_url: &str, api_key: Option<&str>) -> Result<Self> {
        let mut network =
            NetworkConfig::from_rpc_url("client", rpc_url.parse().context("Invalid NEAR RPC URL")?);

        // Pass any API key as an `Authorization: Bearer` header rather than in the URL.
        // near_api's openapi client appends the RPC path after the full base URL, so a
        // key in the query string (`?apiKey=KEY`) becomes `?apiKey=KEY/` and is rejected.
        if let Some(key) = api_key.filter(|key| !key.is_empty()) {
            let url = network.rpc_endpoints[0].url.clone();
            network.rpc_endpoints = vec![RPCEndpoint::new(url).with_api_key(key.to_string())];
        }

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
        with_retries(&format!("get_bridged_token({chain:?})"), || {
            let contract = self.omni_bridge.clone();
            let network = self.network.clone();
            let token_address = token_address.clone();
            async move {
                let result: Data<Option<OmniAddress>> = contract
                    .call_function(
                        "get_bridged_token",
                        json!({ "address": token_address, "chain": chain }),
                    )
                    .read_only()
                    .fetch_from(&network)
                    .await
                    .with_context(|| {
                        format!("Failed to fetch bridged token ({token_address}) on {chain:?}")
                    })?;
                Ok(result.data)
            }
        })
        .await
    }

    /// Current on-chain locked amount for `(chain_kind, token_id)`, if any.
    pub async fn get_locked_tokens(
        &self,
        chain_kind: ChainKind,
        token_id: &AccountId,
    ) -> Result<Option<u128>> {
        with_retries(&format!("get_locked_tokens({chain_kind:?})"), || {
            let contract = self.omni_bridge.clone();
            let network = self.network.clone();
            let token_id = token_id.clone();
            async move {
                let result: Data<Option<U128>> = contract
                    .call_function(
                        "get_locked_tokens",
                        json!({ "chain_kind": chain_kind, "token_id": token_id }),
                    )
                    .read_only()
                    .fetch_from(&network)
                    .await
                    .with_context(|| {
                        format!("Failed to fetch locked tokens for ({chain_kind:?}, {token_id})")
                    })?;
                Ok(result.data.map(|amount| amount.0))
            }
        })
        .await
    }

    /// NEP-141 `ft_balance_of(account_id)` on `token_contract` (used to read the
    /// bridge's custody balance of a NEAR-origin token).
    pub async fn ft_balance_of(
        &self,
        token_contract: &AccountId,
        account_id: &AccountId,
    ) -> Result<u128> {
        with_retries("ft_balance_of", || {
            let network = self.network.clone();
            let token_contract = token_contract.clone();
            let account_id = account_id.clone();
            async move {
                let balance: Data<U128> = Contract(token_contract.clone())
                    .call_function("ft_balance_of", json!({ "account_id": account_id }))
                    .read_only()
                    .fetch_from(&network)
                    .await
                    .with_context(|| {
                        format!("Failed to fetch ft_balance_of({account_id}) on {token_contract}")
                    })?;
                Ok(balance.data.0)
            }
        })
        .await
    }

    /// Resolve the token's NEAR account id, or `None` if it has no NEAR representation.
    async fn resolve_near_account(&self, token_address: OmniAddress) -> Result<Option<AccountId>> {
        match token_address {
            OmniAddress::Near(token_id) => Ok(Some(token_id)),
            address => match self.get_bridged_token(&address, ChainKind::Near).await? {
                Some(OmniAddress::Near(token_id)) => Ok(Some(token_id)),
                Some(other) => bail!("Unexpected address type ({other}) for Near chain"),
                None => Ok(None),
            },
        }
    }

    async fn ft_decimals(&self, token_id: &AccountId) -> Result<u8> {
        with_retries("ft_metadata", || {
            let network = self.network.clone();
            let token_id = token_id.clone();
            async move {
                let metadata: Data<FtMetadataDecimals> = Contract(token_id.clone())
                    .call_function("ft_metadata", ())
                    .read_only()
                    .fetch_from(&network)
                    .await
                    .with_context(|| format!("Failed to fetch ft_metadata on {token_id}"))?;
                Ok(metadata.data.decimals)
            }
        })
        .await
    }

    async fn ft_total_supply(&self, token_id: &AccountId) -> Result<u128> {
        with_retries("ft_total_supply", || {
            let network = self.network.clone();
            let token_id = token_id.clone();
            async move {
                let total: Data<U128> = Contract(token_id.clone())
                    .call_function("ft_total_supply", ())
                    .read_only()
                    .fetch_from(&network)
                    .await
                    .with_context(|| format!("Failed to fetch ft_total_supply on {token_id}"))?;
                Ok(total.data.0)
            }
        })
        .await
    }

    /// Send a single `set_locked_tokens` transaction with the given batch of args.
    /// Returns an error (without panicking) if the transaction executes with failure.
    /// Not auto-retried — re-running the tool is the safe way to retry a write.
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
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<super::TokenSupply>> {
        let Some(token_id) = self.resolve_near_account(token_address).await? else {
            return Ok(None);
        };

        let amount = self.ft_total_supply(&token_id).await?;
        let decimals = self.ft_decimals(&token_id).await?;
        Ok(Some(super::TokenSupply { amount, decimals }))
    }

    async fn get_decimals(&self, token_address: OmniAddress) -> Result<Option<u8>> {
        let Some(token_id) = self.resolve_near_account(token_address).await? else {
            return Ok(None);
        };
        Ok(Some(self.ft_decimals(&token_id).await?))
    }
}

/// Minimal view of NEP-148 `ft_metadata` — we only need `decimals` (extra fields ignored).
#[derive(serde::Deserialize)]
struct FtMetadataDecimals {
    decimals: u8,
}

/// Retry `op` on transient transport errors with exponential backoff (250ms, 500ms, 1s).
/// Genuine errors (e.g. `UnknownAccount`) are returned immediately, not retried.
async fn with_retries<T, F, Fut>(label: &str, mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                attempt += 1;
                if attempt >= MAX_READ_ATTEMPTS || !is_transient(&err) {
                    return Err(err);
                }
                let backoff = Duration::from_millis(250 * u64::from(1u32 << (attempt - 1)));
                eprintln!(
                    "Retrying {label} after transient error (attempt {attempt}/{MAX_READ_ATTEMPTS}): {err:#}"
                );
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

/// near_api marks connection/transport errors as "critical" and won't retry them, but
/// they're usually transient (a dropped connection under concurrent load). Match them by
/// message so we retry only those — not genuine errors like `UnknownAccount`.
fn is_transient(err: &anyhow::Error) -> bool {
    let message = format!("{err:#}").to_lowercase();
    [
        "communication error",
        "error sending request",
        "timed out",
        "timeout",
        "connection reset",
        "connection closed",
        "connection refused",
        "transporterror",
        "dispatch task is gone",
        "channel closed",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}
