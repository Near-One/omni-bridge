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

const ETH_INIT_TRANSFER_GAS: u128 = 100_000;
const ETH_FIN_TRANSFER_GAS: u128 = 150_000;

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

pub async fn get_price_by_symbol(symbol: &str) -> Result<f64> {
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

pub async fn get_price_by_contract_address(platform: &str, address: &str) -> Result<f64> {
    let url =  format!("https://api.coingecko.com/api/v3/simple/token_price/{platform}?contract_addresses={address}&vs_currencies=usd");

    let response = reqwest::get(&url).await?;
    let json = response
        .json::<HashMap<String, HashMap<String, f64>>>()
        .await?;

    json.get(address)
        .and_then(|inner_map| inner_map.get("usd").copied())
        .ok_or_else(|| anyhow::anyhow!("Failed to get price for address: {}", address))
}

pub async fn is_fee_sufficient(
    config: &config::Config,
    jsonrpc_client: &JsonRpcClient,
    sender: &OmniAddress,
    recipient: &OmniAddress,
    token: &AccountId,
    fee: &Fee,
) -> Result<bool> {
    let (sender_symbol, sender_token_decimals) = match sender.get_chain() {
        ChainKind::Eth => ("ethereum", 18),
        ChainKind::Near => ("near", 24),
        ChainKind::Sol => ("solana", 9),
        ChainKind::Arb => ("arbitrum", 18),
        ChainKind::Base => ("base", 18),
    };
    let (recipient_symbol, recipient_token_decimals) = match recipient.get_chain() {
        ChainKind::Eth => ("ethereum", 18),
        ChainKind::Near => ("near", 24),
        ChainKind::Sol => ("solana", 9),
        ChainKind::Arb => ("arbitrum", 18),
        ChainKind::Base => ("base", 18),
    };

    let native_fee_usdt = fee.native_fee.0 as f64 / 10u128.pow(sender_token_decimals) as f64
        * get_price_by_symbol(sender_symbol).await?;

    let fee_token_decimals = get_token_decimals(jsonrpc_client, token).await?;
    let token_fee_usdt = fee.fee.0 as f64 / 10u128.pow(fee_token_decimals) as f64
        * get_price_by_contract_address("near-protocol", token.as_ref()).await?;

    let expected_sender_fee_usdt = match sender {
        OmniAddress::Eth(_) => {
            let http_provider = ProviderBuilder::new().on_http(
                config
                    .evm
                    .rpc_http_url
                    .parse()
                    .context("Failed to parse ETH rpc provider as url")?,
            );
            (ETH_INIT_TRANSFER_GAS * http_provider.get_gas_price().await?) as f64
                / 10u128.pow(sender_token_decimals) as f64
                * get_price_by_symbol(sender_symbol).await?
        }
        OmniAddress::Near(ref address) => {
            if !storage::is_storage_sufficient(
                jsonrpc_client,
                token,
                &address.parse::<AccountId>()?,
            )
            .await?
            {
                storage::NEP141_STORAGE_DEPOSIT as f64 / 10u128.pow(sender_token_decimals) as f64
                    * get_price_by_symbol(sender_symbol).await?
            } else {
                0.0
            }
        }
        OmniAddress::Sol(_) => todo!(),
        OmniAddress::Arb(_) => todo!(),
        OmniAddress::Base(_) => todo!(),
    };

    let expected_recipient_fee_usdt = match recipient {
        OmniAddress::Eth(_) => {
            let http_provider = ProviderBuilder::new().on_http(
                config
                    .evm
                    .rpc_http_url
                    .parse()
                    .context("Failed to parse ETH rpc provider as url")?,
            );
            (ETH_FIN_TRANSFER_GAS * http_provider.get_gas_price().await?) as f64
                / 10u128.pow(recipient_token_decimals) as f64
                * get_price_by_symbol(recipient_symbol).await?
        }
        OmniAddress::Near(ref address) => {
            if !storage::is_storage_sufficient(
                jsonrpc_client,
                token,
                &address.parse::<AccountId>()?,
            )
            .await?
            {
                storage::NEP141_STORAGE_DEPOSIT as f64 / 10u128.pow(recipient_token_decimals) as f64
                    * get_price_by_symbol(recipient_symbol).await?
            } else {
                0.0
            }
        }
        OmniAddress::Sol(_) => todo!(),
        OmniAddress::Arb(_) => todo!(),
        OmniAddress::Base(_) => todo!(),
    };

    Ok(native_fee_usdt + token_fee_usdt >= expected_sender_fee_usdt + expected_recipient_fee_usdt)
}
