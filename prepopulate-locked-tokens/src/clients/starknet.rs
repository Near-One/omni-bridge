use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use omni_types::{ChainKind, H256, OmniAddress};
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use std::sync::Arc;

/// Reads Cairo ERC-20 views (`total_supply`, `balance_of`) via the `starknet_call`
/// JSON-RPC method.
///
/// We talk to the node directly (reqwest) instead of using the `starknet` crate: its
/// `Felt`/lambdaworks-math backend requires rustc 1.87+, while the rest of the workspace
/// builds on 1.86. All we need from Starknet are read-only calls.
pub struct Client {
    near_client: Arc<super::near::Client>,
    http: reqwest::Client,
    rpc_url: String,
    chain: ChainKind,
}

impl Client {
    pub fn new(
        near_client: Arc<super::near::Client>,
        rpc_http_url: &str,
        chain: ChainKind,
    ) -> Result<Self> {
        Ok(Self {
            near_client,
            http: reqwest::Client::new(),
            rpc_url: rpc_http_url.to_string(),
            chain,
        })
    }

    /// Cairo ERC-20 `total_supply` of a contract.
    pub async fn total_supply(&self, contract: &H256) -> Result<u128> {
        self.call_u256(contract, "total_supply", &[]).await
    }

    /// Cairo ERC-20 `balance_of(account)` (the bridge's custody balance of a
    /// Starknet-origin token).
    pub async fn balance_of(&self, contract: &H256, account: &H256) -> Result<u128> {
        let account_hex = felt_hex(account);
        self.call_u256(contract, "balance_of", &[account_hex]).await
    }

    /// Invoke a Cairo view returning a `u256` (low, high felts) via `starknet_call`.
    async fn call_u256(&self, contract: &H256, entry_point: &str, calldata: &[String]) -> Result<u128> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "starknet_call",
            "params": {
                "request": {
                    "contract_address": felt_hex(contract),
                    "entry_point_selector": format!("0x{}", hex::encode(starknet_keccak(entry_point))),
                    "calldata": calldata,
                },
                "block_id": "latest"
            }
        });

        let response: Value = self
            .http
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to send starknet_call ({entry_point})"))?
            .error_for_status()
            .context("Starknet RPC returned an error status")?
            .json()
            .await
            .context("Failed to parse Starknet RPC response")?;

        if let Some(error) = response.get("error") {
            bail!("Starknet RPC error ({entry_point}): {error}");
        }

        let result = response
            .get("result")
            .and_then(Value::as_array)
            .context("Missing `result` array in Starknet response")?;

        decode_u256(result)
    }
}

#[async_trait]
impl super::Client for Client {
    async fn get_total_supply(&self, token_address: OmniAddress) -> Result<Option<u128>> {
        let address = match token_address {
            OmniAddress::Strk(addr) if self.chain == ChainKind::Strk => addr,
            address => match self.near_client.get_bridged_token(&address, self.chain).await? {
                Some(OmniAddress::Strk(addr)) => addr,
                Some(other) => bail!("Unexpected address type ({other}) for {:?} chain", self.chain),
                None => return Ok(None),
            },
        };

        Ok(Some(self.total_supply(&address).await?))
    }
}

fn felt_hex(value: &H256) -> String {
    format!("0x{}", hex::encode(value.0))
}

/// The Starknet entry-point selector is `starknet_keccak(name)`: keccak256 masked to
/// 250 bits (clear the top 6 bits of the big-endian hash).
fn starknet_keccak(name: &str) -> [u8; 32] {
    let mut hash: [u8; 32] = Keccak256::digest(name.as_bytes()).into();
    hash[0] &= 0x03;
    hash
}

/// A Cairo `u256` is returned as two felts: `[low, high]` (each a 128-bit limb).
/// The on-chain guard stores `u128`, so a non-zero `high` is an error.
fn decode_u256(felts: &[Value]) -> Result<u128> {
    let [low, high] = felts else {
        bail!("Unexpected u256 return shape: {felts:?}");
    };
    if felt_to_u128(high)? != 0 {
        bail!("Starknet u256 value exceeds u128");
    }
    felt_to_u128(low)
}

fn felt_to_u128(felt: &Value) -> Result<u128> {
    let felt = felt.as_str().context("Starknet felt is not a string")?;
    let hex = felt.strip_prefix("0x").unwrap_or(felt);
    u128::from_str_radix(hex, 16).with_context(|| format!("Felt {felt} does not fit into u128"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn starknet_keccak_is_250_bit_and_deterministic() {
        let selector = starknet_keccak("total_supply");
        // Top 6 bits must be cleared to fit a 250-bit field element.
        assert_eq!(selector[0] & 0xfc, 0);
        assert_eq!(selector, starknet_keccak("total_supply"));
        // A different name yields a different selector.
        assert_ne!(selector, starknet_keccak("totalSupply"));
    }

    #[test]
    fn selectors_match_known_values() {
        // Cross-check against get_selector_from_name(...) (same constants the
        // omni-bridge-monitor uses).
        assert_eq!(
            format!("0x{}", hex::encode(starknet_keccak("total_supply"))),
            "0x01557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836"
        );
        assert_eq!(
            format!("0x{}", hex::encode(starknet_keccak("balance_of"))),
            "0x035a73cd311a05d46deda634c5ee045db92f811b4e74bca4437fcb5302b7af33"
        );
    }

    #[test]
    fn decodes_u256_low_high_pair() {
        assert_eq!(decode_u256(&[json!("0xa"), json!("0x0")]).unwrap(), 10);
        let max = json!(format!("0x{:x}", u128::MAX));
        assert_eq!(decode_u256(&[max, json!("0x0")]).unwrap(), u128::MAX);
    }

    #[test]
    fn rejects_amount_above_u128() {
        assert!(decode_u256(&[json!("0x0"), json!("0x1")]).is_err());
    }
}
