use std::sync::Arc;

use anyhow::Result;
use bridge_indexer_types::documents_types::DepositMsg;
use futures::future::join_all;
use rust_decimal::MathematicalOps;
use tracing::warn;

use ethereum_types::H256;

use near_jsonrpc_client::JsonRpcClient;
use near_sdk::json_types::U128;
use solana_sdk::pubkey::Pubkey;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, Fee, OmniAddress, TransferId, TransferMessage, near_events::OmniBridgeEvent,
};
use utxo_utils::address::UTXOChain;

use crate::{config, utils};

mod evm;
mod near;
mod solana;
pub mod utxo;

const PAUSED_ERROR: u32 = 6008;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RetryableEvent<E> {
    pub event: E,
    pub creation_timestamp: i64,
    pub last_updated_timestamp: i64,
    pub retries: u32,
}

impl<E> RetryableEvent<E> {
    pub fn new(event: E) -> Self {
        let current_timestamp = chrono::Utc::now().timestamp();

        Self {
            event,
            creation_timestamp: current_timestamp,
            last_updated_timestamp: current_timestamp,
            retries: 0,
        }
    }
}

pub enum EventAction {
    Retry,
    Remove,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "init_transfer")]
pub enum Transfer {
    Near {
        transfer_message: TransferMessage,
    },
    Evm {
        chain_kind: ChainKind,
        tx_hash: H256,
        log: utils::evm::InitTransferMessage,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        amount: U128,
        token: Pubkey,
        sender: OmniAddress,
        recipient: OmniAddress,
        fee: U128,
        native_fee: u64,
        message: String,
        emitter: Pubkey,
        sequence: u64,
    },
    NearToUtxo {
        chain: UTXOChain,
        btc_pending_id: String,
        sign_index: u64,
    },
    UtxoToNear {
        chain: UTXOChain,
        btc_tx_hash: String,
        vout: u64,
        deposit_msg: DepositMsg,
    },
    Fast {
        block_number: u64,
        tx_hash: String,
        token: String,
        amount: U128,
        transfer_id: TransferId,
        recipient: OmniAddress,
        fee: Fee,
        msg: String,
        storage_deposit_amount: Option<U128>,
        safe_confirmations: u64,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "fin_transfer")]
pub enum FinTransfer {
    Evm {
        chain_kind: ChainKind,
        tx_hash: H256,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        emitter: String,
        sequence: u64,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "deploy_token")]
pub enum DeployToken {
    Evm {
        chain_kind: ChainKind,
        tx_hash: H256,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        emitter: String,
        sequence: u64,
    },
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub async fn process_events(
    config: config::Config,
    redis_connection_manager: redis::aio::ConnectionManager,
    omni_connector: Arc<OmniConnector>,
    fast_connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
    near_fast_nonce: Option<Arc<utils::nonce::NonceManager>>,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> Result<()> {
    let signer = omni_connector
        .near_bridge_client()
        .and_then(near_bridge_client::NearBridgeClient::account_id)?;

    loop {
        let mut redis_connection_manager_clone = redis_connection_manager.clone();

        let Some(retryable_events) = utils::redis::get_events(
            &config,
            &mut redis_connection_manager_clone,
            utils::redis::EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.redis.sleep_time_after_events_process_secs,
            ))
            .await;
            continue;
        };

        if retryable_events.is_empty() {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                config.redis.sleep_time_after_events_process_secs,
            ))
            .await;
            continue;
        }

        if let Err(err) = near_omni_nonce.resync_nonce().await {
            warn!("Failed to resync near nonce: {err:?}");
        }

        if let Some(near_fast_nonce) = near_fast_nonce.clone() {
            if let Err(err) = near_fast_nonce.resync_nonce().await {
                warn!("Failed to resync near fast nonce: {err:?}");
            }
        }

        if let Err(err) = evm_nonces.resync_nonces().await {
            warn!("Failed to resync evm nonces: {err:?}");
        }

        let current_timestamp = chrono::Utc::now().timestamp();

        let mut events = Vec::new();

        for (key, payload) in retryable_events {
            let mut retryable_event =
                match serde_json::from_str::<RetryableEvent<serde_json::Value>>(&payload) {
                    Ok(retryable_event) => retryable_event,
                    Err(err) => {
                        warn!("Failed to deserialize retryable event: {err:?}");
                        utils::redis::remove_event(
                            &config,
                            &mut redis_connection_manager_clone,
                            utils::redis::EVENTS,
                            &key,
                        )
                        .await;
                        continue;
                    }
                };

            if current_timestamp - retryable_event.creation_timestamp
                > config.redis.keep_transfers_for_secs
            {
                warn!(
                    "Event ({payload}) with key {key} has exceeded the retention period, removing it"
                );
                utils::redis::remove_event(
                    &config,
                    &mut redis_connection_manager_clone,
                    utils::redis::EVENTS,
                    &key,
                )
                .await;
                continue;
            }

            let delay = i64::try_from(
                config
                    .redis
                    .sleep_time_after_events_process_secs
                    .saturating_mul(
                        config
                            .redis
                            .fee_retry_base_secs
                            .powu(u64::from(retryable_event.retries))
                            .try_into()
                            .unwrap_or(u64::MAX),
                    ),
            )
            .unwrap_or(i64::MAX)
            .min(config.redis.fee_retry_max_sleep_secs);

            if current_timestamp < retryable_event.last_updated_timestamp + delay {
                continue;
            }

            retryable_event.retries += 1;
            retryable_event.last_updated_timestamp = current_timestamp;

            utils::redis::add_event(
                &config,
                &mut redis_connection_manager_clone,
                utils::redis::EVENTS,
                &key,
                &retryable_event,
            )
            .await;

            events.push((key, retryable_event.event));
        }

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(transfer) = serde_json::from_value::<Transfer>(event.clone()) {
                if let Transfer::Near {
                    transfer_message, ..
                } = transfer.clone()
                {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let signer = signer.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match near::process_transfer_event(
                                &config,
                                &mut redis_connection_manager,
                                key.clone(),
                                omni_connector,
                                signer,
                                transfer,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&transfer_message.get_transfer_id())
                                            .unwrap_or_default(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Evm {
                    log, chain_kind, ..
                } = transfer.clone()
                {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_omni_nonce = near_omni_nonce.clone();

                        async move {
                            match evm::process_init_transfer_event(
                                &config,
                                &mut redis_connection_manager,
                                omni_connector,
                                transfer,
                                near_omni_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&TransferId {
                                            origin_nonce: log.origin_nonce,
                                            origin_chain: chain_kind,
                                        })
                                        .unwrap_or_default(),
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&TransferId {
                                            origin_nonce: log.origin_nonce,
                                            origin_chain: chain_kind,
                                        })
                                        .unwrap_or_default(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Solana { sequence, .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let key = key.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_init_transfer_event(
                                &config,
                                &mut redis_connection_manager,
                                key.clone(),
                                omni_connector,
                                transfer,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&TransferId {
                                            origin_nonce: sequence,
                                            origin_chain: ChainKind::Sol,
                                        })
                                        .unwrap_or_default(),
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&TransferId {
                                            origin_nonce: sequence,
                                            origin_chain: ChainKind::Sol,
                                        })
                                        .unwrap_or_default(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::NearToUtxo { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_omni_nonce = near_omni_nonce.clone();

                        async move {
                            match utxo::process_near_to_utxo_init_transfer_event(
                                omni_connector,
                                transfer,
                                near_omni_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::UtxoToNear { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match utxo::process_utxo_to_near_init_transfer_event(
                                omni_connector,
                                transfer,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Fast { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let fast_connector = fast_connector.clone();
                        let near_omni_nonce = near_omni_nonce.clone();

                        async move {
                            match near::initiate_fast_transfer(
                                fast_connector,
                                transfer,
                                near_omni_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(omni_bridge_event) =
                serde_json::from_value::<OmniBridgeEvent>(event.clone())
            {
                if let OmniBridgeEvent::SignTransferEvent {
                    message_payload, ..
                } = omni_bridge_event.clone()
                {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let signer = signer.clone();
                        let evm_nonces = evm_nonces.clone();

                        async move {
                            match near::process_sign_transfer_event(
                                &config,
                                &mut redis_connection_manager,
                                omni_connector,
                                signer,
                                omni_bridge_event,
                                evm_nonces,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&message_payload.transfer_id)
                                            .unwrap_or_default(),
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::FEE_MAPPING,
                                        serde_json::to_string(&message_payload.transfer_id)
                                            .unwrap_or_default(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(fin_transfer_event) =
                serde_json::from_value::<FinTransfer>(event.clone())
            {
                if let FinTransfer::Evm { .. } = fin_transfer_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match evm::process_evm_transfer_event(
                                omni_connector,
                                fin_transfer_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let FinTransfer::Solana { .. } = fin_transfer_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_fin_transfer_event(
                                &config,
                                omni_connector,
                                fin_transfer_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(deploy_token_event) =
                serde_json::from_value::<DeployToken>(event.clone())
            {
                if let DeployToken::Evm { .. } = deploy_token_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match evm::process_deploy_token_event(
                                omni_connector,
                                deploy_token_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let DeployToken::Solana { .. } = deploy_token_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection_manager = redis_connection_manager.clone();
                        let omni_connector = omni_connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_deploy_token_event(
                                &config,
                                omni_connector,
                                deploy_token_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &config,
                                        &mut redis_connection_manager,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(sign_btc_transaction_event) =
                serde_json::from_value::<utxo::SignUtxoTransaction>(event.clone())
            {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection_manager = redis_connection_manager.clone();
                    let omni_connector = omni_connector.clone();

                    async move {
                        match utxo::process_sign_transaction_event(
                            omni_connector,
                            sign_btc_transaction_event,
                        )
                        .await
                        {
                            Ok(EventAction::Retry) => {}
                            Ok(EventAction::Remove) => {
                                utils::redis::remove_event(
                                    &config,
                                    &mut redis_connection_manager,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                warn!("{err:?}");
                                utils::redis::remove_event(
                                    &config,
                                    &mut redis_connection_manager,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                        }
                    }
                }));
            } else if let Ok(confirmed_tx_hash) =
                serde_json::from_value::<utxo::ConfirmedTxHash>(event.clone())
            {
                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection_manager = redis_connection_manager.clone();
                    let omni_connector = omni_connector.clone();
                    let near_nonce = near_omni_nonce.clone();

                    async move {
                        match utxo::process_confirmed_tx_hash(
                            omni_connector,
                            confirmed_tx_hash,
                            near_nonce,
                        )
                        .await
                        {
                            Ok(EventAction::Retry) => {}
                            Ok(EventAction::Remove) => {
                                utils::redis::remove_event(
                                    &config,
                                    &mut redis_connection_manager,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                warn!("{err:?}");
                                utils::redis::remove_event(
                                    &config,
                                    &mut redis_connection_manager,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                        }
                    }
                }));
            } else if let Ok(unverified_event) =
                serde_json::from_value::<near::UnverifiedTrasfer>(event.clone())
            {
                tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection_manager = redis_connection_manager.clone();
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        near::process_unverified_transfer_event(
                            &config,
                            &mut redis_connection_manager,
                            jsonrpc_client,
                            unverified_event,
                        )
                        .await;
                    }
                });
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            config.redis.sleep_time_after_events_process_secs,
        ))
        .await;
    }
}
