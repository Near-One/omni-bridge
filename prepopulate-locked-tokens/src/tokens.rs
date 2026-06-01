use anyhow::{Context, Result};
use near_api::AccountId;
use omni_types::ChainKind;
use serde::Deserialize;

/// A single token as reported by the bridge API (`/api/v3/tokens`).
///
/// `origin_chain` is the authoritative origin, replacing the old account-id prefix
/// heuristic. `decimals` / `origin_decimals` mirror the contract's `Decimals` and are
/// needed to convert a destination's `total_supply` into the origin-decimals unit that
/// `locked_tokens` is stored in (see the contract's `denormalize_amount`). The API
/// returns more fields (`token_address`, `name`, `symbol`) which serde ignores.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub token_id: AccountId,
    pub origin_chain: ChainKind,
    /// Normalized decimals: the decimals of the cross-chain wire format and of the
    /// EVM/SVM/Starknet representations. Note the NEAR representation of a foreign-origin
    /// token instead uses `origin_decimals` (see `main::locked_value`).
    pub decimals: u8,
    /// Origin-chain decimals. `null`/absent means it equals `decimals` (no scaling).
    #[serde(default)]
    pub origin_decimals: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct TokensResponse {
    tokens: Vec<TokenInfo>,
}

/// Fetch the full token list from the bridge API.
pub async fn fetch_tokens(api_url: &str) -> Result<Vec<TokenInfo>> {
    let response = reqwest::get(api_url)
        .await
        .with_context(|| format!("Failed to request tokens from {api_url}"))?
        .error_for_status()
        .with_context(|| format!("Tokens API returned an error status ({api_url})"))?;

    let body: TokensResponse = response
        .json()
        .await
        .context("Failed to parse tokens API response")?;

    Ok(body.tokens)
}
