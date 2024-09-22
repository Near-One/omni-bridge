use std::collections::HashMap;

use near_primitives::types::AccountId;
use omni_types::OmniAddress;

pub async fn get_price_by_symbol(symbol: &str) -> Option<f64> {
    let url =
        format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd");

    let response = reqwest::get(&url).await.ok()?;
    let json = response.json::<HashMap<String, HashMap<String, f64>>>().await.ok()?;

    json.get(symbol)?.get("usd").copied()
}

pub async fn get_price_by_contract_address(platform: &str, address: &str) -> Option<f64> {
    let url = 
        format!("https://api.coingecko.com/api/v3/simple/token_price/{platform}?contract_addresses={address}&vs_currencies=usd");

    let response = reqwest::get(&url).await.ok()?;
    let json = response.json::<HashMap<String, HashMap<String, f64>>>().await.ok()?;

    json.get(address)?.get("usd").copied()
}

pub async fn is_fee_sufficient(sender: &OmniAddress, recipient: &OmniAddress, token: &AccountId, fee: u128) -> Option<bool> {
    // TODO: This feels odd
    // 1 NEAR is 10^24 yoctoNEAR, so it'd be necessary to divide final result by 10^24, 
    // but this logic can't be applied to every NEP141 token
    let token_price = get_price_by_contract_address("near-protocol", token.as_ref()).await?;
    let given_fee = fee as f64 * token_price;

    // TODO: Right now I chose a random fee (around 0.10 USD). It should be calculated based on the chain
    let sender_fee = match sender {
        OmniAddress::Near(_) => 0.03 * get_price_by_symbol("near").await?,
        OmniAddress::Eth(_) => 0.00005 * get_price_by_symbol("ethereum").await?,
        OmniAddress::Sol(_) => 0.001 * get_price_by_symbol("sol").await?
    };
    let recipient_fee = match recipient {
        OmniAddress::Near(_) => 0.03 * get_price_by_symbol("near").await?,
        OmniAddress::Eth(_) => 0.00005 * get_price_by_symbol("ethereum").await?,
        OmniAddress::Sol(_) => 0.001 * get_price_by_symbol("sol").await?
    };

    Some(sender_fee + recipient_fee <= given_fee)
}
