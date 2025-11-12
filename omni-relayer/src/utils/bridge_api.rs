use alloy::primitives::U256;
use anyhow::{Context, Result};
use near_sdk::json_types::U128;
use omni_types::{Fee, OmniAddress, TransferId};
use tracing::{info, warn};

use crate::{config, utils, workers::EventAction};

#[allow(clippy::struct_field_names)]
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct TransferFee {
    pub native_token_fee: Option<U128>,
    pub transferred_token_fee: Option<U128>,
    pub usd_fee: f64,
}

impl PartialEq for TransferFee {
    fn eq(&self, other: &Self) -> bool {
        self.usd_fee.total_cmp(&other.usd_fee) == std::cmp::Ordering::Equal
    }
}

impl Eq for TransferFee {}

impl PartialOrd for TransferFee {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TransferFee {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.usd_fee.total_cmp(&other.usd_fee) {
            std::cmp::Ordering::Equal => match self.native_token_fee.cmp(&other.native_token_fee) {
                std::cmp::Ordering::Equal => {
                    self.transferred_token_fee.cmp(&other.transferred_token_fee)
                }
                other => other,
            },
            other => other,
        }
    }
}

impl TransferFee {
    pub fn apply_discount(&self, discount_percent: u8) -> Self {
        let discount_multiplier = (100 - discount_percent) as f64 / 100.0;

        Self {
            native_token_fee: self
                .native_token_fee
                .map(|fee| U128((fee.0 as f64 * discount_multiplier).ceil() as u128)),
            transferred_token_fee: self
                .transferred_token_fee
                .map(|fee| U128((fee.0 as f64 * discount_multiplier).ceil() as u128)),
            usd_fee: self.usd_fee * discount_multiplier,
        }
    }

    pub async fn get_transfer_fee(
        config: &config::Config,
        sender: &OmniAddress,
        recipient: &OmniAddress,
        token: &OmniAddress,
    ) -> Result<Self> {
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
        reqwest::get(&url).await?.json().await.map_err(Into::into)
    }

    pub fn is_fee_sufficient(&self, config: &config::Config, provided_fee: &Fee) -> bool {
        let (native_fee, transferred_fee) = {
            let discounted_fee = self.apply_discount(config.bridge_indexer.fee_discount);
            (
                discounted_fee.native_token_fee.unwrap_or_default().0,
                discounted_fee.transferred_token_fee.unwrap_or_default().0,
            )
        };

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
        &self,
        config: &config::Config,
        redis_connection_manager: &mut redis::aio::ConnectionManager,
        transfer: &T,
        transfer_id: TransferId,
        provided_fee: &Fee,
    ) -> Option<EventAction> {
        if !self.is_fee_sufficient(config, provided_fee) {
            if provided_fee == &Fee::default() {
                info!("No fee provided for transfer: {transfer:?}, skipping transfer");
                return Some(EventAction::Remove);
            }

            let Ok(transfer_id) = serde_json::to_string(&transfer_id) else {
                warn!("Failed to serialize transfer id: {transfer_id:?}");
                return Some(EventAction::Remove);
            };

            if let Some(historical_fee) =
                utils::redis::get_fee(config, redis_connection_manager, &transfer_id).await
            {
                if historical_fee.is_fee_sufficient(config, provided_fee) {
                    info!(
                        "Historical fee is sufficient for transfer: {transfer:?}, using historical fee: {historical_fee:?}"
                    );
                } else {
                    warn!(
                        "Insufficient fee for transfer: {transfer:?}\nGot: {provided_fee:?}, required: {:?}",
                        self.clone()
                            .min(historical_fee)
                            .apply_discount(config.bridge_indexer.fee_discount)
                    );
                    return Some(EventAction::Retry);
                }
            } else {
                utils::redis::add_event(
                    config,
                    redis_connection_manager,
                    utils::redis::FEE_MAPPING,
                    transfer_id,
                    self,
                )
                .await;
                warn!(
                    "Insufficient fee for transfer: {transfer:?}\nGot: {provided_fee:?}, required: {:?}",
                    self.apply_discount(config.bridge_indexer.fee_discount)
                );
                return Some(EventAction::Retry);
            }
        }

        None
    }
}
