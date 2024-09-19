use std::collections::HashMap;

pub mod near;
pub mod redis;

pub async fn get_price(symbol: &str) -> Option<f64> {
    let url =
        format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd");

    let response = reqwest::get(&url).await.ok()?;
    let json: HashMap<String, HashMap<String, f64>> = response.json().await.ok()?;

    json.get(symbol)?.get("usd").copied()
}
