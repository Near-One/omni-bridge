use std::collections::HashMap;

use alloy::providers::{Provider, ProviderBuilder};
use anyhow::{Context, Result};

use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{
    types::{AccountId, BlockReference},
    views::QueryRequest,
};
use omni_types::{ChainKind, Fee, OmniAddress};
use serde_json::from_slice;

use crate::config;

use super::storage;

#[derive(Debug, serde::Deserialize)]
struct Metadata {
    pub decimals: u32,
}

async fn get_token_decimals(jsonrpc_client: &JsonRpcClient, token: &AccountId) -> Result<u32> {
    let request = methods::query::RpcQueryRequest {
        block_reference: BlockReference::latest(),
        request: QueryRequest::CallFunction {
            account_id: token.clone(),
            method_name: "ft_metadata".to_string(),
            args: Vec::new().into(),
        },
    };

    let response = jsonrpc_client.call(request).await?;

    if let QueryResponseKind::CallResult(result) = response.kind {
        Ok(from_slice::<Metadata>(&result.result)?.decimals)
    } else {
        anyhow::bail!("Failed to get token decimals")
    }
}

async fn get_price_by_symbol(symbol: &str) -> Result<f64> {
    let url =
        format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd");

    let response = reqwest::get(&url).await?;
    let json = response
        .json::<HashMap<String, HashMap<String, f64>>>()
        .await?;

    json.get(symbol)
        .and_then(|inner_map| inner_map.get("usd").copied())
        .ok_or_else(|| anyhow::anyhow!("Failed to get price for symbol: {}", symbol))
}

fn get_symbol_and_decimals_by_chain(chain: ChainKind) -> (&'static str, u32) {
    match chain {
        ChainKind::Eth => ("ethereum", 18),
        ChainKind::Near => ("near", 24),
        ChainKind::Sol => ("solana", 9),
        ChainKind::Arb => ("arbitrum", 18),
        ChainKind::Base => ("base", 18),
    }
}

async fn calculate_price(amount: u128, symbol: &str, decimals: u32) -> Result<f64> {
    Ok(amount as f64 / 10u128.pow(decimals) as f64 * get_price_by_symbol(symbol).await?)
}

pub async fn is_fee_sufficient(
    config: &config::Config,
    jsonrpc_client: &JsonRpcClient,
    sender: &OmniAddress,
    recipient: &OmniAddress,
    token: &AccountId,
    fee: &Fee,
) -> Result<bool> {
    let (sender_symbol, sender_token_decimals) =
        get_symbol_and_decimals_by_chain(sender.get_chain());
    let (recipient_symbol, recipient_token_decimals) =
        get_symbol_and_decimals_by_chain(recipient.get_chain());

    let native_fee_usd =
        calculate_price(fee.native_fee.0, sender_symbol, sender_token_decimals).await?;

    let fee_token_decimals = get_token_decimals(jsonrpc_client, token).await?;
    let token_fee_usd = calculate_price(fee.fee.0, token.as_ref(), fee_token_decimals).await?;

    let expected_recipient_fee_usd = match recipient {
        OmniAddress::Eth(_) => {
            let http_provider = ProviderBuilder::new().on_http(
                config
                    .evm
                    .rpc_http_url
                    .parse()
                    .context("Failed to parse ETH rpc provider as url")?,
            );
            calculate_price(
                config.evm.fin_transfer_gas_estimation as u128
                    * http_provider.get_gas_price().await?,
                recipient_symbol,
                recipient_token_decimals,
            )
            .await?
        }
        OmniAddress::Near(address) => {
            if !storage::has_storage_deposit(jsonrpc_client, token, &address.parse::<AccountId>()?)
                .await?
            {
                calculate_price(
                    storage::NEP141_STORAGE_DEPOSIT,
                    recipient_symbol,
                    recipient_token_decimals,
                )
                .await?
            } else {
                0.0
            }
        }
        OmniAddress::Sol(_) => todo!(),
        OmniAddress::Arb(_) => todo!(),
        OmniAddress::Base(_) => todo!(),
    };

    Ok(native_fee_usd + token_fee_usd >= expected_recipient_fee_usd)
}
