use std::sync::Arc;

use alloy::primitives::B256;
use anyhow::Result;
use bridge_indexer_types::documents_types::DepositMsg;
use futures::future::join_all;
use log::warn;

use ethereum_types::H256;

use near_jsonrpc_client::JsonRpcClient;
use near_sdk::json_types::U128;
use solana_sdk::pubkey::Pubkey;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, Fee, OmniAddress, TransferId, TransferMessage, near_events::OmniBridgeEvent,
};

use crate::{config, utils};

pub mod btc;
mod evm;
mod near;
mod solana;

const PAUSED_ERROR: u32 = 6008;

pub enum EventAction {
    Retry,
    Remove,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "init_transfer")]
pub enum Transfer {
    Near {
        transfer_message: TransferMessage,
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
    },
    Evm {
        chain_kind: ChainKind,
        block_number: u64,
        tx_hash: H256,
        log: utils::evm::InitTransferMessage,
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
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
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
    },
    NearToBtc {
        btc_pending_id: String,
        sign_index: u64,
    },
    BtcToNear {
        btc_tx_hash: String,
        vout: u64,
        deposit_msg: DepositMsg,
    },
    Fast {
        block_number: u64,
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
        block_number: u64,
        tx_hash: H256,
        topic: B256,
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
        block_number: u64,
        tx_hash: H256,
        topic: B256,
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
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    near_fast_bridge_client: Option<Arc<near_bridge_client::NearBridgeClient>>,
    jsonrpc_client: JsonRpcClient,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
    near_fast_nonce: Option<Arc<utils::nonce::NonceManager>>,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let signer = connector
        .near_bridge_client()
        .and_then(near_bridge_client::NearBridgeClient::account_id)?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();

        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

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

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(transfer) = serde_json::from_str::<Transfer>(&event) {
                if let Transfer::Near { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let signer = signer.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match near::process_transfer_event(
                                config,
                                &mut redis_connection,
                                key.clone(),
                                connector,
                                signer,
                                transfer,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Evm { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_fast_bridge_client = near_fast_bridge_client.clone();
                        let jsonrpc_client = jsonrpc_client.clone();
                        let near_omni_nonce = near_omni_nonce.clone();
                        let near_fast_nonce = near_fast_nonce.clone();

                        async move {
                            match evm::process_init_transfer_event(
                                config,
                                connector,
                                near_fast_bridge_client,
                                jsonrpc_client,
                                transfer,
                                near_omni_nonce,
                                near_fast_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Solana { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let key = key.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_init_transfer_event(
                                config,
                                &mut redis_connection,
                                key.clone(),
                                connector,
                                transfer,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::NearToBtc { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match btc::process_near_to_btc_init_transfer_event(
                                connector, transfer, near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::BtcToNear { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match btc::process_btc_to_near_init_transfer_event(
                                connector, transfer, near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                } else if let Transfer::Fast { .. } = transfer {
                    let Some(near_fast_bridge_client) = near_fast_bridge_client.clone() else {
                        warn!(
                            "Fast transfer event received, but near fast bridge connector is not available"
                        );
                        continue;
                    };
                    let Some(near_fast_nonce) = near_fast_nonce.clone() else {
                        warn!(
                            "Fast transfer event received, but near fast nonce manager is not available"
                        );
                        continue;
                    };

                    handlers.push(tokio::spawn({
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_fast_bridge_client = near_fast_bridge_client.clone();
                        let near_fast_nonce = near_fast_nonce.clone();

                        async move {
                            match near::process_fast_transfer_event(
                                connector,
                                near_fast_bridge_client,
                                &key,
                                transfer,
                                near_fast_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(omni_bridge_event) = serde_json::from_str::<OmniBridgeEvent>(&event) {
                if let OmniBridgeEvent::SignTransferEvent { .. } = omni_bridge_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let signer = signer.clone();
                        let evm_nonces = evm_nonces.clone();

                        async move {
                            match near::process_sign_transfer_event(
                                config,
                                connector,
                                signer,
                                omni_bridge_event,
                                evm_nonces,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(fin_transfer_event) = serde_json::from_str::<FinTransfer>(&event) {
                if let FinTransfer::Evm { .. } = fin_transfer_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let jsonrpc_client = jsonrpc_client.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match evm::process_evm_transfer_event(
                                config,
                                connector,
                                jsonrpc_client,
                                fin_transfer_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
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
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_fin_transfer_event(
                                config,
                                connector,
                                fin_transfer_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                        }
                    }));
                }
            } else if let Ok(deploy_token_event) = serde_json::from_str::<DeployToken>(&event) {
                if let DeployToken::Evm { .. } = deploy_token_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let jsonrpc_client = jsonrpc_client.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match evm::process_deploy_token_event(
                                config,
                                connector,
                                jsonrpc_client,
                                deploy_token_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
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
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_omni_nonce.clone();

                        async move {
                            match solana::process_deploy_token_event(
                                config,
                                connector,
                                deploy_token_event,
                                near_nonce,
                            )
                            .await
                            {
                                Ok(EventAction::Retry) => {}
                                Ok(EventAction::Remove) => {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::EVENTS,
                                        &key,
                                    )
                                    .await;
                                }
                                Err(err) => {
                                    warn!("{err:?}");
                                    utils::redis::remove_event(
                                        &mut redis_connection,
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
                serde_json::from_str::<btc::SignBtcTransaction>(&event)
            {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        match btc::process_sign_transaction_event(
                            connector,
                            sign_btc_transaction_event,
                        )
                        .await
                        {
                            Ok(EventAction::Retry) => {}
                            Ok(EventAction::Remove) => {
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                warn!("{err:?}");
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                        }
                    }
                }));
            } else if let Ok(confirmed_tx_hash) =
                serde_json::from_str::<btc::ConfirmedTxHash>(&event)
            {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();
                    let near_nonce = near_omni_nonce.clone();

                    async move {
                        match btc::process_confirmed_tx_hash(
                            connector,
                            confirmed_tx_hash.btc_tx_hash,
                            near_nonce,
                        )
                        .await
                        {
                            Ok(EventAction::Retry) => {}
                            Ok(EventAction::Remove) => {
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                warn!("{err:?}");
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::EVENTS,
                                    &key,
                                )
                                .await;
                            }
                        }
                    }
                }));
            } else if let Ok(unverified_event) =
                serde_json::from_str::<near::UnverifiedTrasfer>(&event)
            {
                tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        near::process_unverified_transfer_event(
                            &mut redis_connection,
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
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
