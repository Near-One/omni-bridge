use anyhow::Result;
use omni_types::{Fee, OmniAddress};

use crate::config;

#[derive(Debug, serde::Deserialize)]
struct TransferFeeResponse {
    native_token_fee: Option<u128>,
    transferred_token_fee: Option<u128>,
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

    let native_fee = response.native_token_fee.unwrap_or_default();
    let transferred_fee = response.transferred_token_fee.unwrap_or_default();

    match (native_fee, transferred_fee) {
        (0, 0) => anyhow::bail!("No fee information found"),
        (0, fee) if fee > 0 => Ok(provided_fee.fee.0 >= fee),
        (native_fee, 0) if native_fee > 0 => Ok(provided_fee.native_fee.0 >= native_fee),
        (native_fee, fee) => {
            let conversion_rate = native_fee as f64 / fee as f64;
            Ok((provided_fee.fee.0 as f64)
                .mul_add(conversion_rate, provided_fee.native_fee.0 as f64)
                >= (fee as f64).mul_add(conversion_rate, native_fee as f64))
        }
    }
}
