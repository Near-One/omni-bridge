use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::{BridgeSdkError, EthRpcError};
use tracing::{info, warn};

use near_bridge_client::{NearBridgeClient, TransactionOptions};
use near_jsonrpc_client::errors::JsonRpcError;
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
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    omni_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Evm {
        chain_kind,
        tx_hash,
        ref log,
        creation_timestamp,
        expected_finalization_time,
    } = transfer
    else {
        anyhow::bail!("Expected EvmInitTransferWithTimestamp, got: {transfer:?}");
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process InitTransfer log on {chain_kind:?}");

    let transfer_id = TransferId {
        origin_chain: chain_kind,
        origin_nonce: log.origin_nonce,
    };

    match omni_connector
        .is_transfer_finalised(Some(chain_kind), ChainKind::Near, log.origin_nonce)
        .await
    {
        Ok(true) => anyhow::bail!("Transfer is already finalised: {transfer_id:?}"),
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
                redis_connection_manager,
                &transfer,
                transfer_id,
                &provided_fee,
            )
            .await
        {
            return Ok(event_action);
        }
    }

    let vaa = if chain_kind == ChainKind::Eth {
        None
    } else if let Ok(vaa) = omni_connector
        .wormhole_get_vaa_by_tx_hash(format!("{tx_hash:?}"))
        .await
    {
        Some(vaa)
    } else {
        warn!("VAA is not ready yet");
        return Ok(EventAction::Retry);
    };

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
        transfer_id: transfer_id.into(),
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
            destination_chain: recipient.get_chain(),
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
            destination_chain: recipient.get_chain(),
            tx_hash,
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
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(JsonRpcError::TransportError(_))
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
            } else if let BridgeSdkError::LightClientNotSynced(block) = err {
                warn!(
                    "Light client is not synced yet for transfer ({}), block: {}",
                    log.origin_nonce, block
                );
                return Ok(EventAction::Retry);
            } else if let BridgeSdkError::EthRpcError(EthRpcError::RpcError(err)) = err {
                warn!(
                    "Ethereum client error occurred while finalizing transfer ({}), retrying: {err:?}",
                    log.origin_nonce
                );
                return Ok(EventAction::Retry);
            }

            anyhow::bail!(
                "Failed to finalize transfer ({}): {err:?}",
                log.origin_nonce
            );
        }
    }
}

pub async fn process_evm_transfer_event(
    omni_connector: Arc<OmniConnector>,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Evm {
        chain_kind,
        tx_hash: transaction_hash,
        creation_timestamp,
        expected_finalization_time,
    } = fin_transfer
    else {
        anyhow::bail!("Expected Evm FinTransfer, got: {fin_transfer:?}");
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process FinTransfer log on {chain_kind:?}");

    let vaa = if chain_kind == ChainKind::Eth {
        None
    } else if let Ok(vaa) = omni_connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
    {
        Some(vaa)
    } else {
        warn!("VAA is not ready yet");
        return Ok(EventAction::Retry);
    };

    let Some(prover_args) = utils::evm::construct_prover_args(
        omni_connector.clone(),
        vaa,
        transaction_hash,
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
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(JsonRpcError::TransportError(_))
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to claim fee, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim fee: {near_rpc_error:?}");
                    }
                };
            } else if let BridgeSdkError::LightClientNotSynced(block) = err {
                warn!("Light client is not synced yet for block: {block}");
                return Ok(EventAction::Retry);
            }

            anyhow::bail!("Failed to claim fee: {err:?}");
        }
    }
}

pub async fn process_deploy_token_event(
    omni_connector: Arc<OmniConnector>,
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Evm {
        chain_kind,
        tx_hash: transaction_hash,
        creation_timestamp,
        expected_finalization_time,
    } = deploy_token_event
    else {
        anyhow::bail!("Expected Evm DeployToken, got: {deploy_token_event:?}");
    };

    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp < creation_timestamp + expected_finalization_time {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process DeployToken log on {chain_kind:?}");

    let vaa = if chain_kind == ChainKind::Eth {
        None
    } else if let Ok(vaa) = omni_connector
        .wormhole_get_vaa_by_tx_hash(format!("{transaction_hash:?}"))
        .await
    {
        Some(vaa)
    } else {
        warn!("VAA is not ready yet");
        return Ok(EventAction::Retry);
    };

    let Some(prover_args) = utils::evm::construct_prover_args(
        omni_connector.clone(),
        vaa,
        transaction_hash,
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
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(JsonRpcError::TransportError(_))
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to bind token, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to bind token: {near_rpc_error:?}");
                    }
                };
            } else if let BridgeSdkError::LightClientNotSynced(block) = err {
                warn!("Light client is not synced yet for block: {block}");
                return Ok(EventAction::Retry);
            }

            anyhow::bail!("Failed to bind token: {err:?}");
        }
    }
}
