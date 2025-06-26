use alloy::primitives::U256;
use anyhow::{Context, Result};
use log::{info, warn};
use near_sdk::json_types::U128;
use omni_types::{Fee, OmniAddress};

use crate::{config, utils, workers::EventAction};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Clone)]
pub struct TransferFeeResponse {
    pub native_token_fee: Option<U128>,
    pub transferred_token_fee: Option<U128>,
    pub usd_fee: f64,
}

pub async fn get_transfer_fee(
    config: &config::Config,
    sender: &OmniAddress,
    recipient: &OmniAddress,
    token: &OmniAddress,
) -> Result<TransferFeeResponse> {
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
    reqwest::get(&url)
        .await?
        .json::<TransferFeeResponse>()
        .await
        .map_err(Into::into)
}

pub async fn is_fee_sufficient(
    config: &config::Config,
    needed_fee: &TransferFeeResponse,
    provided_fee: &Fee,
) -> bool {
    let native_fee = needed_fee.native_token_fee.unwrap_or_default().0
        * u128::from(100 - config.bridge_indexer.fee_discount)
        / 100;
    let transferred_fee = needed_fee.transferred_token_fee.unwrap_or_default().0
        * u128::from(100 - config.bridge_indexer.fee_discount)
        / 100;

    match (native_fee, transferred_fee) {
        (0, 0) => true,
        (0, fee) if fee > 0 => provided_fee.fee.0 >= fee,
        (native_fee, 0) if native_fee > 0 => provided_fee.native_fee.0 >= native_fee,
        (native_fee, fee) => {
            U256::from(provided_fee.fee.0) * U256::from(native_fee)
                + U256::from(provided_fee.native_fee.0) * U256::from(fee)
                >= U256::from(fee) * U256::from(native_fee)
        }
    }
}

pub async fn check_fee<T: std::fmt::Debug>(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    transfer: &T,
    origin_nonce: u64,
    needed_fee: &TransferFeeResponse,
    provided_fee: &Fee,
) -> Option<EventAction> {
    if !is_fee_sufficient(config, needed_fee, provided_fee).await {
        match utils::redis::get_fee(redis_connection, origin_nonce).await {
            Some(historical_fee) => {
                if utils::bridge_api::is_fee_sufficient(config, &historical_fee, provided_fee).await
                {
                    info!(
                        "Historical fee is sufficient for transfer: {transfer:?}, using historical fee: {historical_fee:?}"
                    );
                } else {
                    warn!("Insufficient fee for transfer: {transfer:?}");
                    return Some(EventAction::Retry);
                }
            }
            None => {
                utils::redis::add_event(
                    redis_connection,
                    utils::redis::FEE_MAPPING,
                    origin_nonce,
                    needed_fee,
                )
                .await;
                warn!("Insufficient fee for transfer: {transfer:?}");
                return Some(EventAction::Retry);
            }
        }
    }

    None
}
