use std::sync::Arc;

use anyhow::Result;
use bridge_connector_common::result::BridgeSdkError;
use futures::future::join_all;
use log::{error, info, warn};

use alloy::rpc::types::{Log, TransactionReceipt};
use ethereum_types::H256;

use near_jsonrpc_client::JsonRpcClient;
use solana_client::rpc_request::RpcResponseErrorData;
use solana_rpc_client_api::{client_error::ErrorKind, request::RpcError};
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

use omni_connector::OmniConnector;
use omni_types::{
    locker_args::ClaimFeeArgs, near_events::OmniBridgeEvent, prover_args::WormholeVerifyProofArgs,
    prover_result::ProofKind, ChainKind, OmniAddress, TransferId,
};

use crate::{config, utils};

const NONCE_ALREADY_USED: u32 = 6003;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub event: OmniBridgeEvent,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn sign_transfer(
    #[cfg(not(feature = "disable_fee_check"))] config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_INIT_TRANSFER_QUEUE.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        for (key, event) in events {
            if let Ok(init_transfer_with_timestamp) =
                serde_json::from_str::<InitTransferWithTimestamp>(&event)
            {
                handlers.push(tokio::spawn({
                    #[cfg(not(feature = "disable_fee_check"))]
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        let current_timestamp = chrono::Utc::now().timestamp();

                        if current_timestamp
                            - init_transfer_with_timestamp
                                .last_update_timestamp
                                .unwrap_or_default()
                            < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
                        {
                            return;
                        }

                        let (OmniBridgeEvent::InitTransferEvent {
                            ref transfer_message,
                        }
                        | OmniBridgeEvent::FinTransferEvent {
                            ref transfer_message,
                        }
                        | OmniBridgeEvent::UpdateFeeEvent {
                            ref transfer_message,
                        }) = init_transfer_with_timestamp.event
                        else {
                            warn!(
                                "Expected InitTransferEvent/FinTransferEvent/UpdateFeeEvent, got: {:?}",
                                event
                            );
                            return;
                        };

                        info!(
                            "Trying to process InitTransferEvent/FinTransferEvent/UpdateFeeEvent",
                        );

                        #[cfg(not(feature = "disable_fee_check"))]
                        match utils::fee::is_fee_sufficient(
                            &config,
                            transfer_message.fee.clone(),
                            &transfer_message.sender,
                            &transfer_message.recipient,
                            &transfer_message.token
                        ).await {
                            Ok(true) => {}
                            Ok(false) => {
                                warn!("Insufficient fee for transfer: {:?}", transfer_message);
                                return;
                            }
                            Err(err) => {
                                warn!("Failed to check fee sufficiency: {}", err);
                                return;
                            }
                        }

                        let Ok(signer) = connector
                            .near_bridge_client()
                            .and_then(|connector| connector.signer().map(|signer| signer.account_id)) else {
                                warn!("Failed to set signer account id as fee recipient");
                                return;
                            };

                        match connector
                            .near_sign_transfer(
                                TransferId {
                                    origin_chain: transfer_message.sender.get_chain(),
                                    origin_nonce: transfer_message.origin_nonce,
                                },
                                Some(signer.clone()),
                                Some(transfer_message.fee.clone()),
                            )
                            .await
                        {
                            Ok(tx_hash) => {
                                info!("Signed transfer: {:?}", tx_hash);
                                if utils::near::is_tx_successful(
                                    &jsonrpc_client,
                                    tx_hash,
                                    signer,
                                    Some(
                                        vec![
                                            "Signature request has already been submitted. Please try again later.".to_string(),
                                            "Signature request has timed out.".to_string()
                                        ]
                                    )
                                ).await {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                                        &key,
                                    )
                                    .await;
                                }
                            }
                            Err(err) => {
                                warn!("Failed to sign transfer: {}", err);
                            }
                        }

                        if current_timestamp - init_transfer_with_timestamp.creation_timestamp
                            > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
                        {
                            warn!(
                                "Removing an old InitTransfer: {:?}",
                                init_transfer_with_timestamp
                            );
                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::NEAR_INIT_TRANSFER_QUEUE,
                                &key,
                            )
                            .await;
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

pub async fn finalize_transfer(
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_SIGN_TRANSFER_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();
        for (key, event) in events {
            if let Ok(event) = serde_json::from_str::<OmniBridgeEvent>(&event) {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let OmniBridgeEvent::SignTransferEvent {
                            message_payload, ..
                        } = &event
                        else {
                            error!("Expected SignTransferEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received SignTransferEvent");

                        let fee_recipient = connector
                            .near_bridge_client()
                            .and_then(|connector| {
                                connector.signer().map(|signer| signer.account_id)
                            })
                            .ok();

                        if message_payload.fee_recipient != fee_recipient {
                            warn!(
                                "Fee recipient mismatch: expected {:?}, got {:?}",
                                fee_recipient, message_payload.fee_recipient
                            );
                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                                &key,
                            )
                            .await;
                            return;
                        }

                        let fin_transfer_args = match message_payload.recipient.get_chain() {
                            ChainKind::Near => {
                                warn!("Near to Near transfers are not supported yet");
                                return;
                            }
                            ChainKind::Eth | ChainKind::Base | ChainKind::Arb => {
                                omni_connector::FinTransferArgs::EvmFinTransfer {
                                    chain_kind: message_payload.recipient.get_chain(),
                                    event,
                                }
                            }
                            ChainKind::Sol => {
                                let OmniAddress::Sol(token) = message_payload.token_address.clone()
                                else {
                                    warn!(
                                        "Expected Sol token address, got: {:?}",
                                        message_payload.token_address
                                    );
                                    return;
                                };

                                omni_connector::FinTransferArgs::SolanaFinTransfer {
                                    event,
                                    solana_token: Pubkey::new_from_array(token.0),
                                }
                            }
                        };

                        match connector.fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                info!("Finalized deposit: {}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                                    &key,
                                )
                                .await;
                            }
                            Err(err) => {
                                if let BridgeSdkError::EvmGasEstimateError(_) = err {
                                    utils::redis::remove_event(
                                        &mut redis_connection,
                                        utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                                        &key,
                                    )
                                    .await;
                                }

                                if let BridgeSdkError::SolanaRpcError(ref client_error) = err {
                                    if let ErrorKind::RpcError(RpcError::RpcResponseError {
                                        data:
                                            RpcResponseErrorData::SendTransactionPreflightFailure(
                                                ref result,
                                            ),
                                        ..
                                    }) = client_error.kind
                                    {
                                        if result.err
                                            == Some(TransactionError::InstructionError(
                                                0,
                                                InstructionError::Custom(NONCE_ALREADY_USED),
                                            ))
                                        {
                                            utils::redis::remove_event(
                                                &mut redis_connection,
                                                utils::redis::NEAR_SIGN_TRANSFER_EVENTS,
                                                &key,
                                            )
                                            .await;
                                        }
                                    }
                                }

                                warn!("Failed to finalize deposit: {}", err);
                            }
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub enum FinTransfer {
    Evm {
        chain_kind: ChainKind,
        block_number: u64,
        log: Log,
        tx_logs: Option<Box<TransactionReceipt>>,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        emitter: String,
        sequence: u64,
    },
}

pub async fn claim_fee(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::FINALIZED_TRANSFERS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(fin_transfer) = serde_json::from_str::<FinTransfer>(&event) {
                handlers.push(tokio::spawn(handle_evm_fin_transfer(
                    config.clone(),
                    connector.clone(),
                    jsonrpc_client.clone(),
                    redis_connection.clone(),
                    key.clone(),
                    fin_transfer.clone(),
                )));
                handlers.push(tokio::spawn(handle_solana_fin_transfer(
                    config.clone(),
                    connector.clone(),
                    redis_connection.clone(),
                    key.clone(),
                    fin_transfer.clone(),
                )));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

async fn handle_evm_fin_transfer(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    mut redis_connection: redis::aio::MultiplexedConnection,
    key: String,
    fin_transfer: FinTransfer,
) {
    let FinTransfer::Evm {
        chain_kind,
        block_number,
        log,
        tx_logs,
        creation_timestamp,
        expected_finalization_time,
    } = fin_transfer
    else {
        return;
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return;
    }

    info!("Trying to process FinTransfer log on {:?}", chain_kind);

    let vaa =
        utils::evm::get_vaa_from_evm_log(connector.clone(), chain_kind, tx_logs, &config).await;

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.clone().map(|eth| eth.light_client) else {
                warn!("Eth chain is not configured or light client account id is missing");
                return;
            };

            let Ok(light_client_latest_block_number) =
                utils::near::get_evm_light_client_last_block_number(&jsonrpc_client, light_client)
                    .await
            else {
                warn!("Failed to get eth light client last block number");
                return;
            };

            if block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                ))
                .await;
                return;
            }
        } else {
            warn!("VAA is not ready yet");
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            return;
        }
    }

    let Some(tx_hash) = log.transaction_hash else {
        warn!("No transaction hash in log: {:?}", log);
        return;
    };

    let Some(topic) = log.topic0() else {
        warn!("No topic0 in log: {:?}", log);
        return;
    };

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let Some(prover_args) = utils::evm::construct_prover_args(
        &config,
        vaa,
        tx_hash,
        H256::from_slice(topic.as_slice()),
        ProofKind::FinTransfer,
    )
    .await
    else {
        warn!("Failed to get prover args");
        return;
    };

    let claim_fee_args = ClaimFeeArgs {
        chain_kind,
        prover_args,
    };

    match connector.near_claim_fee(claim_fee_args).await {
        Ok(tx_hash) => {
            info!("Claimed fee: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::FINALIZED_TRANSFERS,
                &key,
            )
            .await;
        }
        Err(err) => {
            warn!("Failed to claim fee: {}", err);
        }
    };
}

async fn handle_solana_fin_transfer(
    config: config::Config,
    connector: Arc<OmniConnector>,
    mut redis_connection: redis::aio::MultiplexedConnection,
    key: String,
    fin_transfer: FinTransfer,
) {
    let FinTransfer::Solana { emitter, sequence } = fin_transfer else {
        return;
    };

    info!("Trying to process FinTransfer log on Solana");

    let Ok(vaa) = connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {}", sequence);
        return;
    };

    let Ok(prover_args) = borsh::to_vec(&WormholeVerifyProofArgs {
        proof_kind: ProofKind::FinTransfer,
        vaa,
    }) else {
        warn!("Failed to serialize prover args to finalize transfer from Solana");
        return;
    };

    let claim_fee_args = ClaimFeeArgs {
        chain_kind: ChainKind::Sol,
        prover_args,
    };

    match connector.near_claim_fee(claim_fee_args).await {
        Ok(tx_hash) => {
            info!("Claimed fee: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::FINALIZED_TRANSFERS,
                &key,
            )
            .await;
        }
        Err(err) => {
            warn!("Failed to claim fee: {}", err);
        }
    };
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub enum DeployToken {
    Evm {
        chain_kind: ChainKind,
        block_number: u64,
        log: Log,
        tx_logs: Option<Box<TransactionReceipt>>,
        creation_timestamp: i64,
        expected_finalization_time: i64,
    },
    Solana {
        emitter: String,
        sequence: u64,
    },
}

pub async fn bind_token(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::DEPLOY_TOKEN_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(deploy_token_event) = serde_json::from_str::<DeployToken>(&event) {
                handlers.push(tokio::spawn(handle_evm_deploy_token_event(
                    config.clone(),
                    connector.clone(),
                    jsonrpc_client.clone(),
                    redis_connection.clone(),
                    key.clone(),
                    deploy_token_event.clone(),
                )));
                handlers.push(tokio::spawn(handle_solana_deploy_token_event(
                    config.clone(),
                    connector.clone(),
                    redis_connection.clone(),
                    key.clone(),
                    deploy_token_event.clone(),
                )));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

async fn handle_evm_deploy_token_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    mut redis_connection: redis::aio::MultiplexedConnection,
    key: String,
    deploy_token_event: DeployToken,
) {
    let DeployToken::Evm {
        chain_kind,
        block_number,
        log,
        tx_logs,
        creation_timestamp,
        expected_finalization_time,
    } = deploy_token_event
    else {
        return;
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return;
    }

    info!("Trying to process DeployToken log on {:?}", chain_kind);

    let vaa =
        utils::evm::get_vaa_from_evm_log(connector.clone(), chain_kind, tx_logs, &config).await;

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.clone().map(|eth| eth.light_client) else {
                warn!("Eth chain is not configured or light client account id is missing");
                return;
            };

            let Ok(light_client_latest_block_number) =
                utils::near::get_evm_light_client_last_block_number(&jsonrpc_client, light_client)
                    .await
            else {
                warn!("Failed to get eth light client last block number");
                return;
            };

            if block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                ))
                .await;
                return;
            }
        } else {
            warn!("VAA is not ready yet");
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            return;
        }
    }

    let Some(tx_hash) = log.transaction_hash else {
        warn!("No transaction hash in log: {:?}", log);
        return;
    };

    let Some(topic) = log.topic0() else {
        warn!("No topic0 in log: {:?}", log);
        return;
    };

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let Some(prover_args) = utils::evm::construct_prover_args(
        &config,
        vaa,
        tx_hash,
        H256::from_slice(topic.as_slice()),
        ProofKind::DeployToken,
    )
    .await
    else {
        warn!("Failed to get prover args");
        return;
    };

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithArgs {
        chain_kind,
        prover_args,
    };

    match connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::DEPLOY_TOKEN_EVENTS,
                &key,
            )
            .await;
        }
        Err(err) => {
            warn!("Failed to bind token: {}", err);
        }
    };
}

async fn handle_solana_deploy_token_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    mut redis_connection: redis::aio::MultiplexedConnection,
    key: String,
    deploy_token_event: DeployToken,
) {
    let DeployToken::Solana { emitter, sequence } = deploy_token_event else {
        return;
    };

    info!("Trying to process DeployToken log on Solana");

    let Ok(vaa) = connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {}", sequence);
        return;
    };

    let Ok(prover_args) = borsh::to_vec(&WormholeVerifyProofArgs {
        proof_kind: ProofKind::DeployToken,
        vaa,
    }) else {
        warn!("Failed to serialize prover args to bind token");
        return;
    };

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithArgs {
        chain_kind: ChainKind::Sol,
        prover_args,
    };

    match connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::DEPLOY_TOKEN_EVENTS,
                &key,
            )
            .await;
        }
        Err(err) => {
            warn!("Failed to bind token: {}", err);
        }
    };
}
