use alloy::primitives::U256;
use anyhow::{Context, Result};
use near_sdk::json_types::U128;
use omni_types::{Fee, OmniAddress, TransferId};

use crate::config;

#[derive(Debug, serde::Deserialize)]
pub struct TransferResponse {
    pub transfer_message: TransferMessage,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransferMessage {
    pub token: OmniAddress,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
}

#[derive(Debug, serde::Deserialize)]
pub struct TransferFeeResponse {
    pub native_token_fee: Option<U128>,
    pub transferred_token_fee: Option<U128>,
}

pub async fn get_transfer(
    config: &config::Config,
    transfer_id: TransferId,
) -> Result<TransferResponse> {
    let url = format!(
        "{}/api/v1/transfers/transfer?origin_chain={:?}&origin_nonce={}",
        config
            .bridge_indexer
            .api_url
            .as_ref()
            .context("No api url was provided")?,
        transfer_id.origin_chain,
        transfer_id.origin_nonce,
    );

    Ok(reqwest::get(&url).await?.json().await?)
}

pub async fn is_fee_sufficient(
    config: &config::Config,
    provided_fee: Fee,
    sender: &OmniAddress,
    recipient: &OmniAddress,
    token: &OmniAddress,
) -> Result<bool> {
    let url = format!(
        "{}/api/v1/transfer-fee?sender={}&recipient={}&token={}",
        config
            .bridge_indexer
            .api_url
            .as_ref()
            .context("No api url was provided")?,
        sender,
        recipient,
        token
    );

    let response = reqwest::get(&url)
        .await?
        .json::<TransferFeeResponse>()
        .await?;

    let native_fee = response.native_token_fee.unwrap_or_default().0;
    let transferred_fee = response.transferred_token_fee.unwrap_or_default().0;

    match (native_fee, transferred_fee) {
        (0, 0) => Ok(true),
        (0, fee) if fee > 0 => {
            Ok(provided_fee.fee.0
                >= fee * u128::from(100 - config.bridge_indexer.fee_discount) / 100)
        }
        (native_fee, 0) if native_fee > 0 => Ok(provided_fee.native_fee.0
            >= native_fee * u128::from(100 - config.bridge_indexer.fee_discount) / 100),
        (native_fee, fee) => Ok(U256::from(provided_fee.fee.0) * U256::from(native_fee)
            + U256::from(provided_fee.native_fee.0) * U256::from(fee)
            >= U256::from(fee)
                * U256::from(native_fee)
                * U256::from(100 - config.bridge_indexer.fee_discount)
                / U256::from(100)),
    }
}
