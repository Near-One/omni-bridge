use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use log::{info, warn};

use ethereum_types::H256;

use near_bridge_client::TransactionOptions;
use near_jsonrpc_client::{JsonRpcClient, errors::JsonRpcError};
use near_primitives::views::TxExecutionStatus;
use near_rpc_client::NearRpcError;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, FastTransfer, Fee, TransferId, locker_args::ClaimFeeArgs, prover_result::ProofKind,
};

use crate::{
    config, utils,
    workers::{DeployToken, FinTransfer},
};

use super::{EventAction, Transfer};

pub async fn process_init_transfer_event(
    config: config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    connector: Arc<OmniConnector>,
    near_fast_bridge_client: Option<Arc<near_bridge_client::NearBridgeClient>>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    transfer: Transfer,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
    near_fast_nonce: Option<Arc<utils::nonce::NonceManager>>,
) -> Result<EventAction> {
    let Transfer::Evm {
        chain_kind,
        block_number,
        tx_hash: transaction_hash,
        ref log,
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

    info!("Trying to process InitTransfer log on {chain_kind:?}");

    let transfer_id = TransferId {
        origin_chain: chain_kind,
        origin_nonce: log.origin_nonce,
    };

    match connector
        .is_transfer_finalised(
            Some(chain_kind),
            log.recipient.get_chain(),
            log.origin_nonce,
        )
        .await
    {
        Ok(true) => anyhow::bail!("Transfer is already finalised: {:?}", transfer_id),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    if config.is_bridge_api_enabled() {
        let sender = utils::evm::string_to_evm_omniaddress(chain_kind, &log.sender.to_string())
            .map_err(|err| {
                anyhow::anyhow!(
                    "Failed to parse \"{}\" as `OmniAddress`: {:?}",
                    log.sender,
                    err
                )
            })?;

        let token =
            utils::evm::string_to_evm_omniaddress(chain_kind, &log.token_address.to_string())
                .map_err(|err| {
                    anyhow::anyhow!(
                        "Failed to parse \"{}\" as `OmniAddress`: {:?}",
                        log.token_address,
                        err
                    )
                })?;

        match utils::bridge_api::is_fee_sufficient(
            &config,
            Fee {
                fee: log.fee,
                native_fee: log.native_fee,
            },
            &sender,
            &log.recipient,
            &token,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                warn!("Insufficient fee for transfer: {transfer:?}");
                return Ok(EventAction::Retry);
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {err:?}");
                return Ok(EventAction::Retry);
            }
        }
    }

    let vaa = connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
        .ok();

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

    let Ok(mut near_bridge_client) = connector.near_bridge_client().cloned() else {
        anyhow::bail!("Failed to get near bridge client");
    };

    let mut nonce = near_omni_nonce;
    if let Some(near_fast_bridge_client) = near_fast_bridge_client {
        let Ok(token_id) = utils::storage::get_token_id(
            &near_fast_bridge_client.clone(),
            transfer_id.origin_chain,
            &log.token_address.to_string(),
        )
        .await
        else {
            warn!("Failed to get token id for transfer: {transfer_id:?}");
            return Ok(EventAction::Retry);
        };

        let fast_transfer_args = FastTransfer {
            transfer_id,
            token_id,
            amount: log.amount,
            fee: Fee {
                fee: log.fee,
                native_fee: log.native_fee,
            },
            recipient: log.recipient.clone(),
            msg: log.message.clone(),
        };

        let Ok(fast_transfer_status) = near_fast_bridge_client
            .get_fast_transfer_status(fast_transfer_args.id())
            .await
        else {
            warn!("Failed to get fast transfer status for transfer: {transfer_id:?}");
            return Ok(EventAction::Retry);
        };

        if let Some(status) = fast_transfer_status {
            let relayer = near_fast_bridge_client.account_id().map_err(|_| {
                anyhow::anyhow!("Failed to get relayer account id for near fast bridge client")
            })?;
            if status.finalised && status.relayer == relayer {
                near_bridge_client = (*near_fast_bridge_client).clone();
                if let Some(near_fast_nonce) = near_fast_nonce {
                    nonce = near_fast_nonce;
                } else {
                    warn!("Near fast nonce is not available, using omni nonce");
                    return Ok(EventAction::Retry);
                }
            }
        }
    }

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &near_bridge_client,
        chain_kind,
        &log.recipient,
        &log.token_address.to_string(),
        log.fee.0,
        log.native_fee.0,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            utils::redis::add_event(redis_connection, utils::redis::EVENTS, key, transfer).await;
            anyhow::bail!("Failed to get storage deposit actions: {}", err);
        }
    };

    let nonce = nonce
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
                wait_final_outcome_timeout_sec: None,
            },
        }
    } else {
        omni_connector::FinTransferArgs::NearFinTransferWithEvmProof {
            chain_kind,
            tx_hash: transaction_hash,
            storage_deposit_actions,
            transaction_options: TransactionOptions {
                nonce: Some(nonce),
                wait_until: TxExecutionStatus::Included,
                wait_final_outcome_timeout_sec: None,
            },
        }
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized InitTransfer: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!(
                            "Failed to finalize transfer ({}), retrying: {near_rpc_error:?}",
                            log.origin_nonce
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to finalize transfer ({}): {near_rpc_error:?}",
                            log.origin_nonce
                        );
                    }
                };
            }
            anyhow::bail!(
                "Failed to finalize transfer ({}): {err:?}",
                log.origin_nonce
            );
        }
    }
}

pub async fn process_evm_transfer_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Evm {
        chain_kind,
        block_number,
        tx_hash: transaction_hash,
        topic,
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

    info!("Trying to process FinTransfer log on {chain_kind:?}");

    let vaa = connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
        .ok();

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

    let Some(prover_args) = utils::evm::construct_prover_args(
        &config,
        vaa,
        transaction_hash,
        H256::from_slice(topic.as_slice()),
        ProofKind::FinTransfer,
    )
    .await
    else {
        warn!("Failed to get prover args for {transaction_hash:?}");
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
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Claimed fee: {tx_hash:?}");
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
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to claim fee, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim fee: {near_rpc_error:?}");
                    }
                };
            }
            anyhow::bail!("Failed to claim fee: {err:?}");
        }
    }
}

pub async fn process_deploy_token_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: JsonRpcClient,
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Evm {
        chain_kind,
        block_number,
        tx_hash: transaction_hash,
        topic,
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

    info!("Trying to process DeployToken log on {chain_kind:?}");

    let vaa = connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
        .ok();

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

    let Some(prover_args) = utils::evm::construct_prover_args(
        &config,
        vaa,
        transaction_hash,
        H256::from_slice(topic.as_slice()),
        ProofKind::DeployToken,
    )
    .await
    else {
        warn!("Failed to get prover args for {transaction_hash:?}");
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
            wait_final_outcome_timeout_sec: None,
        },
    };

    match connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {tx_hash:?}");
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
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to bind token, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to bind token: {near_rpc_error:?}");
                    }
                };
            }

            anyhow::bail!("Failed to bind token: {err:?}");
        }
    }
}
