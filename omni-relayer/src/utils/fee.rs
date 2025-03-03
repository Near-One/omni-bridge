use alloy::primitives::U256;
use anyhow::Result;
use near_sdk::json_types::U128;
use omni_types::{Fee, OmniAddress};

use crate::config;

#[derive(Debug, serde::Deserialize)]
struct TransferFeeResponse {
    native_token_fee: Option<U128>,
    transferred_token_fee: Option<U128>,
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
        config.bridge_indexer.api_url, sender, recipient, token
    );

    let response = reqwest::get(&url)
        .await?
        .json::<TransferFeeResponse>()
        .await?;

    let native_fee = response.native_token_fee.unwrap_or_default().0;
    let transferred_fee = response.transferred_token_fee.unwrap_or_default().0;

    match (native_fee, transferred_fee) {
        (0, 0) => anyhow::bail!("No fee information found"),
        (0, fee) if fee > 0 => Ok(provided_fee.fee.0 >= fee),
        (native_fee, 0) if native_fee > 0 => Ok(provided_fee.native_fee.0 >= native_fee),
        (native_fee, fee) => Ok(U256::from(provided_fee.fee.0) * U256::from(native_fee)
            + U256::from(provided_fee.native_fee.0) * U256::from(fee)
            >= U256::from(fee) * U256::from(native_fee)),
    }
}
