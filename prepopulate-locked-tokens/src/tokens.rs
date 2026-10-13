use anyhow::{Context, Result};
use near_api::AccountId;
use omni_types::ChainKind;
use serde::Deserialize;

/// A single token as reported by the bridge API (`/api/v3/tokens`).
///
/// `origin_chain` is the authoritative origin, replacing the old account-id prefix
/// heuristic. Decimals are NOT taken from here — each representation's decimals are read
/// on-chain per chain (they differ: EVM 18, Solana ~9, …). The API returns more fields
/// (`token_address`, `name`, `symbol`, `decimals`, `origin_decimals`) which serde ignores.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenInfo {
    pub token_id: AccountId,
    pub origin_chain: ChainKind,
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
