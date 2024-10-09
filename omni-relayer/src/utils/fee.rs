use std::collections::HashMap;

use anyhow::Result;

use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{types::{AccountId, BlockReference}, views::QueryRequest};
use omni_types::OmniAddress;
use serde_json::from_slice;

#[derive(Debug, serde::Deserialize)]
pub struct Metadata {
    pub decimals: u32,
}

pub async fn get_token_decimals(jsonrpc_client: &JsonRpcClient, token: &AccountId) -> Result<u32> {
    let request = methods::query::RpcQueryRequest {
        block_reference: BlockReference::latest(),
        request: QueryRequest::CallFunction {
            account_id: token.clone(),
            method_name: "ft_metadata".to_string(),
            args: Vec::new().into()
        },
    };

    let response = jsonrpc_client.call(request).await?;

    if let QueryResponseKind::CallResult(result) = response.kind {
        Ok(from_slice::<Metadata>(&result.result)?.decimals)
    } else {
        anyhow::bail!("Failed to get token decimals")
    }
}

pub async fn get_price_by_symbol(symbol: &str) -> Result<f64> {
    let url =
        format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd");

    let response = reqwest::get(&url).await?;
    let json = response.json::<HashMap<String, HashMap<String, f64>>>().await?;

    json.get(symbol)
        .and_then(|inner_map| inner_map.get("usd").copied())
        .ok_or_else(|| anyhow::anyhow!("Failed to get price for symbol: {}", symbol))
}

pub async fn get_price_by_contract_address(platform: &str, address: &str) -> Result<f64> {
    let url = 
        format!("https://api.coingecko.com/api/v3/simple/token_price/{platform}?contract_addresses={address}&vs_currencies=usd");

    let response = reqwest::get(&url).await?;
    let json = response.json::<HashMap<String, HashMap<String, f64>>>().await?;

    json.get(address)
        .and_then(|inner_map| inner_map.get("usd").copied())
        .ok_or_else(|| anyhow::anyhow!("Failed to get price for address: {}", address))
}

pub async fn is_fee_sufficient(jsonrpc_client: &JsonRpcClient, sender: &OmniAddress, recipient: &OmniAddress, token: &AccountId, fee: u128) -> Result<bool> {
    //let token_price = get_price_by_contract_address("near-protocol", token.as_ref()).await?;
    //let token_decimals = get_token_decimals(jsonrpc_client, token).await?;
    //
    //let given_fee = fee as f64 / 10u128.pow(token_decimals) as f64 * token_price;
    //
    //// TODO: Right now I chose a random fee (around 0.10 USD), but it should be calculated based on the chain in the future
    //let sender_fee = match sender {
    //    OmniAddress::Near(_) => 0.03 * get_price_by_symbol("near").await?,
    //    OmniAddress::Eth(_) => 0.00005 * get_price_by_symbol("ethereum").await?,
    //    OmniAddress::Sol(_) => 0.001 * get_price_by_symbol("solana").await?,
    //    OmniAddress::Arb(_) | OmniAddress::Base(_) => todo!()
    //};
    //let recipient_fee = match recipient {
    //    OmniAddress::Near(_) => 0.03 * get_price_by_symbol("near").await?,
    //    OmniAddress::Eth(_) => 0.00005 * get_price_by_symbol("ethereum").await?,
    //    OmniAddress::Sol(_) => 0.001 * get_price_by_symbol("solana").await?,
    //    OmniAddress::Arb(_) | OmniAddress::Base(_) => todo!()
    //};
    //
    Ok(true)
}
