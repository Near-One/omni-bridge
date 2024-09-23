use anyhow::Result;

use alloy::primitives::Address;

#[derive(Debug, serde::Deserialize)]
struct WormholeApiResponse {
    data: WormholeApiData,
}

#[derive(Debug, serde::Deserialize)]
struct WormholeApiData {
    vaa: String,
}

pub async fn get_vaa(chain_id: u64, emitter: Address, sequence: u64) -> Result<String> {
    let url = format!("https://api.wormholescan.io/api/v1/vaas/{chain_id}/{emitter}/{sequence}");

    let response = reqwest::get(url).await?;
    Ok(response.json::<WormholeApiResponse>().await?.data.vaa)
}
