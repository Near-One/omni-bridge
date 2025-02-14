#[cfg(not(feature = "disable_fee_check"))]
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use futures::future::join_all;
use log::{info, warn};

use alloy::rpc::types::{Log, TransactionReceipt};
use ethereum_types::H256;

use near_bridge_client::TransactionOptions;
use near_jsonrpc_client::JsonRpcClient;
use near_primitives::{hash::CryptoHash, types::AccountId, views::TxExecutionStatus};
use near_rpc_client::NearRpcError;
use near_sdk::json_types::U128;
use solana_client::rpc_request::RpcResponseErrorData;
use solana_rpc_client_api::{client_error::ErrorKind, request::RpcError};
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

use omni_connector::OmniConnector;
#[cfg(not(feature = "disable_fee_check"))]
use omni_types::Fee;
use omni_types::{
    locker_args::ClaimFeeArgs, near_events::OmniBridgeEvent, prover_args::WormholeVerifyProofArgs,
    prover_result::ProofKind, ChainKind, OmniAddress, TransferId,
};

use crate::{config, utils};

const PAUSED_ERROR: u32 = 6008;

enum EventAction {
    Retry,
    Remove,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "init_transfer")]
pub enum Transfer {
    Near {
        event: OmniBridgeEvent,
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
    },
    Evm {
        chain_kind: ChainKind,
        block_number: u64,
        log: Log,
        tx_logs: Option<Box<TransactionReceipt>>,
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
        expected_finalization_time: i64,
    },
    Solana {
        amount: U128,
        token: String,
        sender: String,
        recipient: String,
        fee: U128,
        native_fee: u64,
        message: String,
        emitter: String,
        sequence: u64,
        creation_timestamp: i64,
        last_update_timestamp: Option<i64>,
    },
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "fin_transfer")]
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

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(tag = "deploy_token")]
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct UnverifiedNearTrasfer {
    tx_hash: CryptoHash,
    signer: AccountId,
    specific_errors: Option<Vec<String>>,
    original_key: String,
    original_event: Transfer,
}

#[allow(clippy::too_many_lines)]
pub async fn process_events(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    near_nonce: Arc<utils::nonce::NonceManager>,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let signer = connector
        .near_bridge_client()
        .and_then(|connector| connector.signer().map(|signer| signer.account_id))?;

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

        if let Err(err) = near_nonce.resync_nonce().await {
            warn!("Failed to resync near nonce: {}", err);
        }

        if let Err(err) = evm_nonces.resync_nonces().await {
            warn!("Failed to resync evm nonces: {}", err);
        }

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(transfer) = serde_json::from_str::<Transfer>(&event) {
                if let Transfer::Near { .. } = transfer {
                    handlers.push(tokio::spawn({
                        #[cfg(not(feature = "disable_fee_check"))]
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let signer = signer.clone();
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_near_transfer_event(
                                #[cfg(not(feature = "disable_fee_check"))]
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
                            };
                        }
                    }));
                } else if let Transfer::Evm { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let key = key.clone();
                        let connector = connector.clone();
                        let jsonrpc_client = jsonrpc_client.clone();
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_evm_init_transfer_event(
                                config,
                                &mut redis_connection,
                                key.clone(),
                                connector,
                                jsonrpc_client,
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
                            };
                        }
                    }));
                } else if let Transfer::Solana { .. } = transfer {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let key = key.clone();
                        let connector = connector.clone();
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_solana_init_transfer_event(
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
                            };
                        }
                    }));
                }
            } else if let Ok(omni_bridge_event) = serde_json::from_str::<OmniBridgeEvent>(&event) {
                if let OmniBridgeEvent::SignTransferEvent { .. } = omni_bridge_event {
                    handlers.push(tokio::spawn({
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let signer = signer.clone();
                        let evm_nonces = evm_nonces.clone();

                        async move {
                            match process_sign_transfer_event(
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
                            };
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
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_evm_fin_transfer_event(
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
                            };
                        }
                    }));
                } else if let FinTransfer::Solana { .. } = fin_transfer_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_solana_fin_transfer_event(
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
                            };
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
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_evm_deploy_token_event(
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
                            };
                        }
                    }));
                } else if let DeployToken::Solana { .. } = deploy_token_event {
                    handlers.push(tokio::spawn({
                        let config = config.clone();
                        let mut redis_connection = redis_connection.clone();
                        let connector = connector.clone();
                        let near_nonce = near_nonce.clone();

                        async move {
                            match process_solana_deploy_token_event(
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
                            };
                        }
                    }));
                }
            } else if let Ok(unverified_event) =
                serde_json::from_str::<UnverifiedNearTrasfer>(&event)
            {
                tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        process_unverified_near_transfer_event(
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

async fn process_near_transfer_event(
    #[cfg(not(feature = "disable_fee_check"))] config: config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    connector: Arc<OmniConnector>,
    signer: AccountId,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Near {
        ref event,
        creation_timestamp,
        last_update_timestamp,
    } = transfer
    else {
        anyhow::bail!("Expected NearTransferWithTimestamp, got: {:?}", transfer);
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp - last_update_timestamp.unwrap_or_default()
        < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
    {
        return Ok(EventAction::Retry);
    }

    let (OmniBridgeEvent::InitTransferEvent {
        ref transfer_message,
    }
    | OmniBridgeEvent::FinTransferEvent {
        ref transfer_message,
    }
    | OmniBridgeEvent::UpdateFeeEvent {
        ref transfer_message,
    }) = event
    else {
        anyhow::bail!(
            "Expected InitTransferEvent/FinTransferEvent/UpdateFeeEvent, got: {:?}",
            event
        );
    };

    info!("Trying to process InitTransferEvent/FinTransferEvent/UpdateFeeEvent",);

    #[cfg(not(feature = "disable_fee_check"))]
    match utils::fee::is_fee_sufficient(
        &config,
        transfer_message.fee.clone(),
        &transfer_message.sender,
        &transfer_message.recipient,
        &transfer_message.token,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => {
            warn!("Insufficient fee for transfer: {:?}", transfer_message);
            return Ok(EventAction::Retry);
        }
        Err(err) => {
            warn!("Failed to check fee sufficiency: {}", err);
            return Ok(EventAction::Retry);
        }
    }

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    match connector
        .near_sign_transfer(
            TransferId {
                origin_chain: transfer_message.sender.get_chain(),
                origin_nonce: transfer_message.origin_nonce,
            },
            Some(signer.clone()),
            Some(transfer_message.fee.clone()),
            TransactionOptions {
                nonce: Some(nonce),
                wait_until: near_primitives::views::TxExecutionStatus::Included,
            },
            None,
        )
        .await
    {
        Ok(tx_hash) => {
            utils::redis::add_event(
                redis_connection,
                utils::redis::EVENTS,
                tx_hash.to_string(),
                UnverifiedNearTrasfer {
                    tx_hash,
                    signer,
                    specific_errors: Some(vec![
                        "Signature request has already been submitted. Please try again later."
                            .to_string(),
                        "Signature request has timed out.".to_string(),
                        "Attached deposit is lower than required".to_string(),
                    ]),
                    original_key: key,
                    original_event: transfer,
                },
            )
            .await;

            info!("Signed transfer: {:?}", tx_hash);

            Ok(EventAction::Remove)
        }
        Err(err) => {
            if current_timestamp - creation_timestamp
                > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
            {
                anyhow::bail!("Transfer is too old");
            }

            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to sign transfer, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to sign transfer: {}", near_rpc_error);
                    }
                };
            }
            anyhow::bail!("Failed to sign transfer: {}", err);
        }
    }
}

async fn process_sign_transfer_event(
    connector: Arc<OmniConnector>,
    signer: AccountId,
    sign_transfer_event: OmniBridgeEvent,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> Result<EventAction> {
    let OmniBridgeEvent::SignTransferEvent {
        message_payload, ..
    } = &sign_transfer_event
    else {
        anyhow::bail!("Expected SignTransferEvent, got: {:?}", sign_transfer_event);
    };

    info!("Received SignTransferEvent");

    if message_payload.fee_recipient != Some(signer) {
        anyhow::bail!("Fee recipient mismatch");
    }

    let fin_transfer_args = match message_payload.recipient.get_chain() {
        ChainKind::Near => {
            anyhow::bail!("Near to Near transfers are not supported yet");
        }
        ChainKind::Eth | ChainKind::Base | ChainKind::Arb => {
            let nonce = evm_nonces
                .reserve_nonce(message_payload.recipient.get_chain())
                .await
                .context("Failed to reserve nonce for evm transaction")?;

            omni_connector::FinTransferArgs::EvmFinTransfer {
                chain_kind: message_payload.recipient.get_chain(),
                event: sign_transfer_event,
                tx_nonce: Some(nonce.into()),
            }
        }
        ChainKind::Sol => {
            let OmniAddress::Sol(token) = message_payload.token_address.clone() else {
                anyhow::bail!(
                    "Expected Sol token address, got: {:?}",
                    message_payload.token_address
                );
            };

            omni_connector::FinTransferArgs::SolanaFinTransfer {
                event: sign_transfer_event,
                solana_token: Pubkey::new_from_array(token.0),
            }
        }
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized deposit: {}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::EvmGasEstimateError(_) = err {
                anyhow::bail!("Failed to finalize deposit: {}", err);
            }

            if let BridgeSdkError::SolanaRpcError(ref client_error) = err {
                if let ErrorKind::RpcError(RpcError::RpcResponseError {
                    data: RpcResponseErrorData::SendTransactionPreflightFailure(ref result),
                    ..
                }) = client_error.kind
                {
                    if let Some(TransactionError::InstructionError(
                        _,
                        InstructionError::Custom(error_code),
                    )) = result.err
                    {
                        if error_code == PAUSED_ERROR {
                            warn!("Solana bridge is paused");
                            return Ok(EventAction::Retry);
                        }

                        anyhow::bail!("Failed to finalize deposit: {err}");
                    }
                }
            }

            Ok(EventAction::Retry)
        }
    }
}

async fn process_evm_init_transfer_event(
    config: config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Evm {
        chain_kind,
        block_number,
        ref log,
        ref tx_logs,
        creation_timestamp,
        last_update_timestamp,
        expected_finalization_time,
    } = transfer
    else {
        anyhow::bail!("Expected EvmInitTransferWithTimestamp, got: {:?}", transfer);
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    if current_timestamp - last_update_timestamp.unwrap_or_default()
        < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
    {
        return Ok(EventAction::Retry);
    }

    let Ok(init_log) = log.log_decode::<utils::evm::InitTransfer>() else {
        anyhow::bail!("Failed to decode log as InitTransfer: {:?}", log);
    };

    info!("Trying to process InitTransfer log on {:?}", chain_kind);

    let Some(tx_hash) = log.transaction_hash else {
        anyhow::bail!("No transaction hash in log: {:?}", log);
    };

    let recipient = init_log
        .inner
        .recipient
        .parse::<OmniAddress>()
        .map_err(|err| {
            anyhow::anyhow!(
                "Failed to parse \"{}\" as `OmniAddress`: {:?}",
                init_log.inner.recipient,
                err
            )
        })?;

    #[cfg(not(feature = "disable_fee_check"))]
    {
        let sender =
            utils::evm::string_to_evm_omniaddress(chain_kind, &init_log.inner.sender.to_string())
                .map_err(|err| {
                anyhow::anyhow!(
                    "Failed to parse \"{}\" as `OmniAddress`: {:?}",
                    init_log.inner.recipient,
                    err
                )
            })?;

        let token = utils::evm::string_to_evm_omniaddress(
            chain_kind,
            &init_log.inner.tokenAddress.to_string(),
        )
        .map_err(|err| {
            anyhow::anyhow!(
                "Failed to parse \"{}\" as `OmniAddress`: {:?}",
                init_log.inner.recipient,
                err
            )
        })?;

        match utils::fee::is_fee_sufficient(
            &config,
            Fee {
                fee: init_log.inner.fee.into(),
                native_fee: init_log.inner.nativeFee.into(),
            },
            &sender,
            &recipient,
            &token,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                warn!("Insufficient fee for transfer: {:?}", transfer);
                return Ok(EventAction::Retry);
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {}", err);
                return Ok(EventAction::Retry);
            }
        }
    }

    let vaa =
        utils::evm::get_vaa_from_evm_log(connector.clone(), chain_kind, tx_logs.clone(), &config)
            .await;

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.map(|eth| eth.light_client) else {
                anyhow::bail!("Eth chain is not configured or light client account id is missing");
            };

            let Ok(light_client_latest_block_number) =
                utils::near::get_evm_light_client_last_block_number(&jsonrpc_client, light_client)
                    .await
            else {
                warn!("Failed to get eth light client last block number");
                return Ok(EventAction::Retry);
            };

            if block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                ))
                .await;
                return Ok(EventAction::Retry);
            }
        } else {
            warn!("VAA is not ready yet");
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            return Ok(EventAction::Retry);
        }
    }

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &connector,
        chain_kind,
        &recipient,
        &init_log.inner.tokenAddress.to_string(),
        init_log.inner.fee,
        init_log.inner.nativeFee,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            utils::redis::add_event(redis_connection, utils::redis::EVENTS, key, transfer).await;
            anyhow::bail!("Failed to get storage deposit actions: {}", err);
        }
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    let fin_transfer_args = if let Some(vaa) = vaa {
        omni_connector::FinTransferArgs::NearFinTransferWithVaa {
            chain_kind,
            storage_deposit_actions,
            vaa,
            transaction_options: TransactionOptions {
                nonce: Some(nonce),
                wait_until: TxExecutionStatus::Included,
            },
            wait_final_outcome_timeout_sec: None,
        }
    } else {
        omni_connector::FinTransferArgs::NearFinTransferWithEvmProof {
            chain_kind,
            tx_hash,
            storage_deposit_actions,
            transaction_options: TransactionOptions {
                nonce: Some(nonce),
                wait_until: TxExecutionStatus::Included,
            },
            wait_final_outcome_timeout_sec: None,
        }
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized InitTransfer: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to finalize transfer, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to finalize transfer: {}", near_rpc_error);
                    }
                };
            }
            anyhow::bail!("Failed to finalize transfer: {}", err);
        }
    }
}

async fn process_solana_init_transfer_event(
    config: config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Solana {
        #[cfg(not(feature = "disable_fee_check"))]
        ref sender,
        ref token,
        ref recipient,
        fee,
        native_fee,
        ref emitter,
        sequence,
        last_update_timestamp,
        ..
    } = transfer
    else {
        anyhow::bail!(
            "Expected SolanaInitTransferWithTimestamp, got: {:?}",
            transfer
        );
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp - last_update_timestamp.unwrap_or_default()
        < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
    {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process InitTransfer log on Solana");

    let recipient = recipient.parse::<OmniAddress>().map_err(|err| {
        anyhow::anyhow!(
            "Failed to parse \"{}\" as `OmniAddress`: {:?}",
            recipient,
            err
        )
    })?;

    #[cfg(not(feature = "disable_fee_check"))]
    {
        let sender = Pubkey::from_str(sender)?;

        let sender =
            OmniAddress::new_from_slice(ChainKind::Sol, &sender.to_bytes()).map_err(|err| {
                anyhow::anyhow!("Failed to parse \"{}\" as `OmniAddress`: {:?}", sender, err)
            })?;

        let token = Pubkey::from_str(token)?;

        let token =
            OmniAddress::new_from_slice(ChainKind::Sol, &token.to_bytes()).map_err(|err| {
                anyhow::anyhow!("Failed to parse \"{}\" as `OmniAddress`: {:?}", sender, err)
            })?;

        match utils::fee::is_fee_sufficient(
            &config,
            Fee {
                fee,
                native_fee: u128::from(native_fee).into(),
            },
            &sender,
            &recipient,
            &token,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                warn!("Insufficient fee for transfer: {:?}", transfer);
                return Ok(EventAction::Retry);
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {}", err);
                return Ok(EventAction::Retry);
            }
        }
    }

    let Ok(vaa) = connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, &emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {}", sequence);
        return Ok(EventAction::Retry);
    };

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &connector,
        ChainKind::Sol,
        &recipient,
        token,
        fee.0,
        u128::from(native_fee),
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            utils::redis::add_event(redis_connection, utils::redis::STUCK_EVENTS, &key, transfer)
                .await;
            anyhow::bail!("Failed to get storage deposit actions: {}", err);
        }
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    let fin_transfer_args = omni_connector::FinTransferArgs::NearFinTransferWithVaa {
        chain_kind: ChainKind::Sol,
        storage_deposit_actions,
        vaa,
        transaction_options: TransactionOptions {
            nonce: Some(nonce),
            wait_until: TxExecutionStatus::Included,
        },
        wait_final_outcome_timeout_sec: None,
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized InitTransfer: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to finalize transfer, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to finalize transfer: {}", near_rpc_error);
                    }
                };
            }

            anyhow::bail!("Failed to finalize transfer: {}", err);
        }
    }
}

async fn process_evm_fin_transfer_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Evm {
        chain_kind,
        block_number,
        log,
        tx_logs,
        creation_timestamp,
        expected_finalization_time,
    } = fin_transfer
    else {
        anyhow::bail!("Expected Evm FinTransfer, got: {:?}", fin_transfer);
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process FinTransfer log on {:?}", chain_kind);

    let vaa =
        utils::evm::get_vaa_from_evm_log(connector.clone(), chain_kind, tx_logs, &config).await;

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.clone().map(|eth| eth.light_client) else {
                anyhow::bail!("Eth chain is not configured or light client account id is missing");
            };

            let Ok(light_client_latest_block_number) =
                utils::near::get_evm_light_client_last_block_number(&jsonrpc_client, light_client)
                    .await
            else {
                warn!("Failed to get eth light client last block number");
                return Ok(EventAction::Retry);
            };

            if block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                return Ok(EventAction::Retry);
            }
        } else {
            warn!("VAA is not ready yet");
            return Ok(EventAction::Retry);
        }
    }

    let Some(tx_hash) = log.transaction_hash else {
        anyhow::bail!("No transaction hash in log: {:?}", log);
    };

    let Some(topic) = log.topic0() else {
        anyhow::bail!("No topic0 in log: {:?}", log);
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
        warn!("Failed to get prover args for {:?}", tx_hash);
        return Ok(EventAction::Retry);
    };

    let claim_fee_args = ClaimFeeArgs {
        chain_kind,
        prover_args,
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    match connector
        .near_claim_fee(
            claim_fee_args,
            TransactionOptions {
                nonce: Some(nonce),
                wait_until: near_primitives::views::TxExecutionStatus::Included,
            },
            None,
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Claimed fee: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if current_timestamp - creation_timestamp
                > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
            {
                anyhow::bail!("Transfer is too old");
            }

            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to claim fee, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim fee: {}", near_rpc_error);
                    }
                };
            }
            anyhow::bail!("Failed to claim fee: {}", err);
        }
    }
}

async fn process_solana_fin_transfer_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Solana { emitter, sequence } = fin_transfer else {
        anyhow::bail!("Expected Solana FinTransfer, got: {:?}", fin_transfer);
    };

    info!("Trying to process FinTransfer log on Solana");

    let Ok(vaa) = connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {}", sequence);
        return Ok(EventAction::Retry);
    };

    let Ok(prover_args) = borsh::to_vec(&WormholeVerifyProofArgs {
        proof_kind: ProofKind::FinTransfer,
        vaa,
    }) else {
        anyhow::bail!("Failed to serialize prover args for {:?}", sequence);
    };

    let claim_fee_args = ClaimFeeArgs {
        chain_kind: ChainKind::Sol,
        prover_args,
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    match connector
        .near_claim_fee(
            claim_fee_args,
            TransactionOptions {
                nonce: Some(nonce),
                wait_until: near_primitives::views::TxExecutionStatus::Included,
            },
            None,
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Claimed fee: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to claim fee, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim fee: {}", near_rpc_error);
                    }
                };
            }
            anyhow::bail!("Failed to claim fee: {}", err);
        }
    }
}

async fn process_evm_deploy_token_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Evm {
        chain_kind,
        block_number,
        log,
        tx_logs,
        creation_timestamp,
        expected_finalization_time,
    } = deploy_token_event
    else {
        anyhow::bail!("Expected Evm DeployToken, got: {:?}", deploy_token_event);
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process DeployToken log on {:?}", chain_kind);

    let vaa =
        utils::evm::get_vaa_from_evm_log(connector.clone(), chain_kind, tx_logs, &config).await;

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.clone().map(|eth| eth.light_client) else {
                anyhow::bail!("Eth chain is not configured or light client account id is missing");
            };

            let Ok(light_client_latest_block_number) =
                utils::near::get_evm_light_client_last_block_number(&jsonrpc_client, light_client)
                    .await
            else {
                warn!("Failed to get eth light client last block number");
                return Ok(EventAction::Retry);
            };

            if block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                return Ok(EventAction::Retry);
            }
        } else {
            warn!("VAA is not ready yet");
            return Ok(EventAction::Retry);
        }
    }

    let Some(tx_hash) = log.transaction_hash else {
        anyhow::bail!("No transaction hash in log: {:?}", log);
    };

    let Some(topic) = log.topic0() else {
        anyhow::bail!("No topic0 in log: {:?}", log);
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
        warn!("Failed to get prover args for {:?}", tx_hash);
        return Ok(EventAction::Retry);
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithArgs {
        chain_kind,
        prover_args,
        transaction_options: TransactionOptions {
            nonce: Some(nonce),
            wait_until: near_primitives::views::TxExecutionStatus::Included,
        },
        wait_final_outcome_timeout_sec: None,
    };

    match connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if current_timestamp - creation_timestamp
                > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
            {
                anyhow::bail!("Transfer is too old");
            }

            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to bind token, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to bind token: {}", near_rpc_error);
                    }
                };
            }

            anyhow::bail!("Failed to bind token: {}", err);
        }
    }
}

async fn process_solana_deploy_token_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Solana { emitter, sequence } = deploy_token_event else {
        anyhow::bail!("Expected Solana DeployToken, got: {:?}", deploy_token_event);
    };

    info!("Trying to process DeployToken log on Solana");

    let Ok(vaa) = connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {}", sequence);
        return Ok(EventAction::Retry);
    };

    let Ok(prover_args) = borsh::to_vec(&WormholeVerifyProofArgs {
        proof_kind: ProofKind::DeployToken,
        vaa,
    }) else {
        anyhow::bail!("Failed to serialize prover args for {:?}", sequence);
    };

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {}", err);
            return Ok(EventAction::Retry);
        }
    };

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithArgs {
        chain_kind: ChainKind::Sol,
        prover_args,
        transaction_options: TransactionOptions {
            nonce,
            wait_until: near_primitives::views::TxExecutionStatus::Included,
        },
        wait_final_outcome_timeout_sec: None,
    };

    match connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {:?}", tx_hash);
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError | NearRpcError::FinalizationError => {
                        warn!("Failed to bind token, retrying: {}", near_rpc_error);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to bind token: {}", near_rpc_error);
                    }
                };
            }

            anyhow::bail!("Failed to bind token: {}", err);
        }
    }
}

async fn process_unverified_near_transfer_event(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: JsonRpcClient,
    unverified_event: UnverifiedNearTrasfer,
) {
    utils::redis::remove_event(
        redis_connection,
        utils::redis::EVENTS,
        unverified_event.tx_hash.to_string(),
    )
    .await;

    if !utils::near::is_tx_successful(
        &jsonrpc_client,
        unverified_event.tx_hash,
        unverified_event.signer,
        unverified_event.specific_errors,
    )
    .await
    {
        utils::redis::add_event(
            redis_connection,
            utils::redis::EVENTS,
            unverified_event.original_key,
            unverified_event.original_event,
        )
        .await;
    }
}
