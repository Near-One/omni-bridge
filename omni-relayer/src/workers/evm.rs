use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use tracing::{info, warn};

use ethereum_types::H256;

use near_bridge_client::{NearBridgeClient, TransactionOptions};
use near_jsonrpc_client::{JsonRpcClient, errors::JsonRpcError};
use near_primitives::views::TxExecutionStatus;
use near_rpc_client::NearRpcError;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, FastTransfer, Fee, OmniAddress, TransferId, locker_args::ClaimFeeArgs,
    prover_result::ProofKind,
};

use crate::{
    config, utils,
    workers::{DeployToken, FinTransfer},
};

use super::{EventAction, Transfer};

#[allow(clippy::too_many_lines)]
pub async fn process_init_transfer_event(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    omni_connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    transfer: Transfer,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
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
        < config.redis.check_insufficient_fee_transfers_every_secs
    {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process InitTransfer log on {chain_kind:?}");

    let transfer_id = TransferId {
        origin_chain: chain_kind,
        origin_nonce: log.origin_nonce,
    };

    match omni_connector
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

        let Ok(needed_fee) = utils::bridge_api::TransferFee::get_transfer_fee(
            config,
            &sender,
            &log.recipient,
            &token,
        )
        .await
        else {
            warn!("Failed to get transfer fee for transfer: {transfer:?}");
            return Ok(EventAction::Retry);
        };

        let provided_fee = Fee {
            fee: log.fee,
            native_fee: log.native_fee,
        };

        if let Some(event_action) = needed_fee
            .check_fee(
                config,
                redis_connection,
                &transfer,
                transfer_id,
                &provided_fee,
            )
            .await
        {
            return Ok(event_action);
        }
    }

    let vaa = omni_connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
        .ok();

    if vaa.is_none() {
        if chain_kind == ChainKind::Eth {
            let Some(Some(light_client)) = config.eth.as_ref().map(|eth| eth.light_client.clone())
            else {
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

    let mut recipient = log.recipient.clone();
    let mut fee_recipient = omni_connector
        .near_bridge_client()
        .and_then(NearBridgeClient::account_id)
        .context("Failed to get relayer account id")?;

    let Ok(token_id) = utils::storage::get_token_id(
        &omni_connector,
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

    let Ok(fast_transfer_status) = omni_connector
        .near_get_fast_transfer_status(fast_transfer_args.id())
        .await
    else {
        warn!("Failed to get fast transfer status for transfer: {transfer_id:?}");
        return Ok(EventAction::Retry);
    };

    if let Some(status) = fast_transfer_status {
        recipient = OmniAddress::Near(status.relayer.clone());
        fee_recipient = status.relayer;
    }

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &omni_connector,
        chain_kind,
        &recipient,
        &fee_recipient,
        &log.token_address.to_string(),
        log.fee.0,
        log.native_fee.0,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            warn!("Failed to get storage deposit actions: {err}");
            return Ok(EventAction::Retry);
        }
    };

    let nonce = near_omni_nonce
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

    match omni_connector.fin_transfer(fin_transfer_args).await {
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
    config: &config::Config,
    omni_connector: Arc<OmniConnector>,
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

    let vaa = omni_connector
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
        config,
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

    match omni_connector
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
                > config.redis.keep_insufficient_fee_transfers_for
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
    config: &config::Config,
    omni_connector: Arc<OmniConnector>,
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

    let vaa = omni_connector
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
        config,
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

    match omni_connector.bind_token(bind_token_args).await {
        Ok(tx_hash) => {
            info!("Bound token: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if current_timestamp - creation_timestamp
                > config.redis.keep_insufficient_fee_transfers_for
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
