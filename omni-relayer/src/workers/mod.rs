use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use alloy::primitives::TxHash;
use anyhow::{Context, Result};
use bridge_indexer_types::documents_types::DepositMsg;
use near_jsonrpc_client::JsonRpcClient;
use near_primitives::types::AccountId;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use near_sdk::json_types::U128;
use solana_sdk::pubkey::Pubkey;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, Fee, OmniAddress, TransferId, TransferMessage, UnifiedTransferId,
    UtxoFinTransferMsg, near_events::OmniBridgeEvent,
};

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

struct MessageResult {
    action: Result<EventAction>,
    needs_evm_nonce_resync: bool,
    fee_key_to_remove: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "init_transfer")]
pub enum Transfer {
    Near {
        transfer_message: TransferMessage,
    },
    Evm {
        chain_kind: ChainKind,
        tx_hash: TxHash,
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
    Utxo {
        utxo_transfer_message: UtxoFinTransferMsg,
        new_transfer_id: UnifiedTransferId,
    },
    NearToUtxo {
        chain: ChainKind,
        btc_pending_id: String,
        sign_index: u64,
    },
    UtxoToNear {
        chain: ChainKind,
        btc_tx_hash: String,
        vout: u32,
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
        tx_hash: TxHash,
        creation_timestamp: i64,
        expected_finalization_time: i64,
        transfer_id: TransferId,
    },
    Solana {
        emitter: String,
        sequence: u64,
        transfer_id: Option<TransferId>,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "deploy_token")]
pub enum DeployToken {
    Evm {
        chain_kind: ChainKind,
        tx_hash: TxHash,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        emitter: String,
        sequence: u64,
    },
}

async fn handle_nats_ack(
    msg: &async_nats::jetstream::message::Message,
    result: &Result<EventAction>,
) {
    match result {
        Ok(EventAction::Retry) => {}
        Ok(EventAction::Remove) => {
            msg.ack().await.ok();
        }
        Err(_) => {
            msg.ack_with(async_nats::jetstream::AckKind::Term)
                .await
                .ok();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process_events(
    config: Arc<config::Config>,
    redis_connection_manager: redis::aio::ConnectionManager,
    nats_client: Arc<utils::nats::NatsClient>,
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

    near_omni_nonce
        .resync_nonce()
        .await
        .context("Failed to resync near nonce")?;

    if let Some(near_fast_nonce) = near_fast_nonce.clone() {
        near_fast_nonce
            .resync_nonce()
            .await
            .context("Failed to resync near fast nonce")?;
    }

    let is_evm_nonce_resync_needed = Arc::new(AtomicBool::new(true));

    let nats_config = config
        .nats
        .as_ref()
        .context("NATS config is required for event processing")?;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(
        nats_config.relayer_consumer.worker_count,
    ));

    info!(
        "Starting event processing with {} concurrent workers",
        nats_config.relayer_consumer.worker_count
    );

    let consumer = nats_client.relayer_consumer(nats_config).await?;
    let mut messages = consumer
        .messages()
        .await
        .context("Failed to start consuming NATS messages")?;

    while let Some(msg) = messages.next().await {
        let msg = msg.context("NATS message error")?;

        if is_evm_nonce_resync_needed.load(Ordering::Relaxed) {
            if let Err(err) = evm_nonces.resync_nonces().await {
                warn!("Failed to resync evm nonces: {err:?}");
                continue;
            }
            is_evm_nonce_resync_needed.store(false, Ordering::Relaxed);
        }

        let event: serde_json::Value = match serde_json::from_slice(&msg.payload) {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to deserialize event: {err:?}");
                msg.ack_with(async_nats::jetstream::AckKind::Term)
                    .await
                    .ok();
                continue;
            }
        };

        let permit = semaphore.clone().acquire_owned().await?;

        let config = config.clone();
        let mut redis = redis_connection_manager.clone();
        let jsonrpc_client = jsonrpc_client.clone();
        let omni_connector = omni_connector.clone();
        let fast_connector = fast_connector.clone();
        let signer = signer.clone();
        let near_omni_nonce = near_omni_nonce.clone();
        let near_fast_nonce = near_fast_nonce.clone();
        let evm_nonces = evm_nonces.clone();
        let is_evm_nonce_resync_needed = is_evm_nonce_resync_needed.clone();

        tokio::spawn(async move {
            let message_result = process_message(
                event,
                &config,
                &mut redis,
                &jsonrpc_client,
                omni_connector,
                fast_connector,
                signer,
                near_omni_nonce,
                near_fast_nonce,
                evm_nonces,
            )
            .await;

            if let Err(ref err) = message_result.action {
                warn!("{err:?}");
            }

            if let Some(fee_key) = message_result.fee_key_to_remove {
                utils::redis::remove_event(&config, &mut redis, utils::redis::FEE_MAPPING, fee_key)
                    .await;
            }

            if message_result.needs_evm_nonce_resync
                && matches!(message_result.action, Ok(EventAction::Retry) | Err(_))
            {
                is_evm_nonce_resync_needed.store(true, Ordering::Relaxed);
            }

            handle_nats_ack(&msg, &message_result.action).await;

            drop(permit);
        });
    }

    Ok(())
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn process_message(
    event: serde_json::Value,
    config: &config::Config,
    redis: &mut redis::aio::ConnectionManager,
    jsonrpc_client: &JsonRpcClient,
    omni_connector: Arc<OmniConnector>,
    fast_connector: Arc<OmniConnector>,
    signer: AccountId,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
    near_fast_nonce: Option<Arc<utils::nonce::NonceManager>>,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> MessageResult {
    if let Ok(transfer) = serde_json::from_value::<Transfer>(event.clone()) {
        match transfer {
            Transfer::Near { .. } | Transfer::Utxo { .. } => {
                let (is_utxo, fee_key) = match &transfer {
                    Transfer::Near { transfer_message } => (
                        transfer_message.recipient.is_utxo_chain(),
                        serde_json::to_string(&transfer_message.get_transfer_id())
                            .unwrap_or_default(),
                    ),
                    Transfer::Utxo {
                        utxo_transfer_message,
                        new_transfer_id,
                    } => (
                        utxo_transfer_message.recipient.is_utxo_chain(),
                        serde_json::to_string(new_transfer_id).unwrap_or_default(),
                    ),
                    _ => unreachable!(),
                };

                let result = if is_utxo {
                    near::process_transfer_to_utxo_event(
                        jsonrpc_client,
                        omni_connector.clone(),
                        transfer,
                        near_omni_nonce.clone(),
                    )
                    .await
                } else {
                    near::process_transfer_event(
                        config,
                        redis,
                        jsonrpc_client,
                        omni_connector.clone(),
                        signer.clone(),
                        transfer,
                        near_omni_nonce.clone(),
                    )
                    .await
                };

                let fee_key_to_remove = result.is_err().then_some(fee_key);
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove,
                }
            }
            Transfer::Evm {
                ref log,
                chain_kind,
                ..
            } => {
                let fee_key = serde_json::to_string(&TransferId {
                    origin_nonce: log.origin_nonce,
                    origin_chain: chain_kind,
                })
                .unwrap_or_default();

                let result = evm::process_init_transfer_event(
                    config,
                    redis,
                    omni_connector.clone(),
                    transfer,
                    near_omni_nonce.clone(),
                )
                .await;

                let fee_key_to_remove =
                    matches!(&result, Ok(EventAction::Remove) | Err(_)).then_some(fee_key);
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove,
                }
            }
            Transfer::Solana { sequence, .. } => {
                let result = solana::process_init_transfer_event(
                    config,
                    redis,
                    omni_connector.clone(),
                    transfer,
                    near_omni_nonce.clone(),
                )
                .await;

                let fee_key = serde_json::to_string(&TransferId {
                    origin_nonce: sequence,
                    origin_chain: ChainKind::Sol,
                })
                .unwrap_or_default();

                let fee_key_to_remove =
                    matches!(&result, Ok(EventAction::Remove) | Err(_)).then_some(fee_key);
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove,
                }
            }
            Transfer::NearToUtxo { .. } => {
                let result = utxo::process_near_to_utxo_init_transfer_event(
                    omni_connector.clone(),
                    transfer,
                    near_omni_nonce.clone(),
                )
                .await;
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove: None,
                }
            }
            Transfer::UtxoToNear { .. } => {
                let result = utxo::process_utxo_to_near_init_transfer_event(
                    omni_connector.clone(),
                    transfer,
                    near_omni_nonce.clone(),
                )
                .await;
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove: None,
                }
            }
            Transfer::Fast { .. } => {
                let Some(near_fast_nonce) = near_fast_nonce.clone() else {
                    return MessageResult {
                        action: Err(anyhow::anyhow!(
                            "Fast transfer event found but near fast nonce manager is not configured"
                        )),
                        needs_evm_nonce_resync: false,
                        fee_key_to_remove: None,
                    };
                };

                let result =
                    near::initiate_fast_transfer(fast_connector.clone(), transfer, near_fast_nonce)
                        .await;
                MessageResult {
                    action: result,
                    needs_evm_nonce_resync: false,
                    fee_key_to_remove: None,
                }
            }
        }
    } else if let Ok(omni_bridge_event) = serde_json::from_value::<OmniBridgeEvent>(event.clone()) {
        if let OmniBridgeEvent::SignTransferEvent {
            ref message_payload,
            ..
        } = omni_bridge_event
        {
            let is_evm = message_payload.recipient.get_chain().is_evm_chain();
            let fee_key = serde_json::to_string(&message_payload.transfer_id).unwrap_or_default();

            let result = near::process_sign_transfer_event(
                config,
                redis,
                omni_connector.clone(),
                signer.clone(),
                omni_bridge_event,
                evm_nonces.clone(),
            )
            .await;

            let fee_key_to_remove =
                matches!(&result, Ok(EventAction::Remove) | Err(_)).then_some(fee_key);
            MessageResult {
                action: result,
                needs_evm_nonce_resync: is_evm,
                fee_key_to_remove,
            }
        } else {
            MessageResult {
                action: Err(anyhow::anyhow!("Unhandled OmniBridgeEvent: {event}")),
                needs_evm_nonce_resync: false,
                fee_key_to_remove: None,
            }
        }
    } else if let Ok(fin_transfer_event) = serde_json::from_value::<FinTransfer>(event.clone()) {
        let result = match fin_transfer_event {
            FinTransfer::Evm { .. } => {
                evm::process_evm_transfer_event(
                    omni_connector.clone(),
                    fin_transfer_event,
                    near_omni_nonce.clone(),
                )
                .await
            }
            FinTransfer::Solana { .. } => {
                solana::process_fin_transfer_event(
                    config,
                    omni_connector.clone(),
                    fin_transfer_event,
                    near_omni_nonce.clone(),
                )
                .await
            }
        };
        MessageResult {
            action: result,
            needs_evm_nonce_resync: false,
            fee_key_to_remove: None,
        }
    } else if let Ok(deploy_token_event) = serde_json::from_value::<DeployToken>(event.clone()) {
        let result = match deploy_token_event {
            DeployToken::Evm { .. } => {
                evm::process_deploy_token_event(
                    omni_connector.clone(),
                    deploy_token_event,
                    near_omni_nonce.clone(),
                )
                .await
            }
            DeployToken::Solana { .. } => {
                solana::process_deploy_token_event(
                    config,
                    omni_connector.clone(),
                    deploy_token_event,
                    near_omni_nonce.clone(),
                )
                .await
            }
        };
        MessageResult {
            action: result,
            needs_evm_nonce_resync: false,
            fee_key_to_remove: None,
        }
    } else if let Ok(sign_utxo_transaction_event) =
        serde_json::from_value::<utxo::SignUtxoTransaction>(event.clone())
    {
        let result = utxo::process_sign_transaction_event(
            omni_connector.clone(),
            sign_utxo_transaction_event,
        )
        .await;
        MessageResult {
            action: result,
            needs_evm_nonce_resync: false,
            fee_key_to_remove: None,
        }
    } else if let Ok(confirmed_tx_hash) =
        serde_json::from_value::<utxo::ConfirmedTxHash>(event.clone())
    {
        let result = utxo::process_confirmed_tx_hash(
            jsonrpc_client,
            omni_connector.clone(),
            confirmed_tx_hash,
            near_omni_nonce.clone(),
        )
        .await;
        MessageResult {
            action: result,
            needs_evm_nonce_resync: false,
            fee_key_to_remove: None,
        }
    } else {
        MessageResult {
            action: Err(anyhow::anyhow!("Unknown event type: {event}")),
            needs_evm_nonce_resync: false,
            fee_key_to_remove: None,
        }
    }
}
