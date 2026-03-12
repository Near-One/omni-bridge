use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use tracing::{info, warn};

use near_bridge_client::{NearBridgeClient, TransactionOptions};
use near_jsonrpc_client::{
    errors::{JsonRpcError, JsonRpcServerError},
    methods::query::RpcQueryError,
};
use near_primitives::views::TxExecutionStatus;
use near_rpc_client::NearRpcError;

use omni_connector::OmniConnector;
use omni_types::{ChainKind, Fee, TransferId};

use crate::{config, utils};

use super::{DeployToken, EventAction, FinTransfer, Transfer};

pub async fn process_init_transfer_event(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    omni_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Starknet {
        ref tx_hash,
        ref sender,
        ref recipient,
        origin_nonce,
        ref token,
        amount: _,
        ref fee,
        ..
    } = transfer
    else {
        anyhow::bail!("Expected StarknetInitTransfer, got: {transfer:?}");
    };

    let transfer_id = TransferId {
        origin_chain: ChainKind::Strk,
        origin_nonce,
    };

    info!(
        "Processing Starknet InitTransfer ({:?}:{}): {tx_hash}",
        transfer_id.origin_chain, transfer_id.origin_nonce
    );

    match omni_connector
        .is_transfer_finalised(Some(ChainKind::Strk), ChainKind::Near, origin_nonce)
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
        let Ok(needed_fee) =
            utils::bridge_api::TransferFee::get_transfer_fee(config, sender, recipient, token)
                .await
        else {
            warn!("Failed to get transfer fee for transfer: {transfer:?}");
            return Ok(EventAction::Retry);
        };

        let provided_fee = Fee {
            fee: fee.fee,
            native_fee: fee.native_fee,
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

    let fee_recipient = omni_connector
        .near_bridge_client()
        .and_then(NearBridgeClient::account_id)
        .context("Failed to get relayer account id")?;

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &omni_connector,
        ChainKind::Strk,
        recipient,
        &fee_recipient,
        &token.to_string(),
        fee.fee.0,
        fee.native_fee.0,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            warn!("Failed to get storage deposit actions: {err}");
            return Ok(EventAction::Retry);
        }
    };

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    let fin_transfer_args = omni_connector::FinTransferArgs::NearFinTransferWithMpcProof {
        chain_kind: ChainKind::Strk,
        destination_chain: recipient.get_chain(),
        storage_deposit_actions,
        tx_hash: tx_hash.clone(),
        transaction_options: TransactionOptions {
            nonce: Some(nonce),
            wait_until: TxExecutionStatus::Included,
            wait_final_outcome_timeout_sec: None,
        },
    };

    match omni_connector.fin_transfer(fin_transfer_args).await {
        Ok(near_tx_hash) => {
            info!(
                "Finalized Starknet InitTransfer ({:?}:{}): {near_tx_hash:?}",
                transfer_id.origin_chain, transfer_id.origin_nonce
            );
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(
                        JsonRpcError::TransportError(_) | JsonRpcError::ServerError(_),
                    )
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!(
                            "Failed to finalize Starknet transfer ({:?}:{}), retrying: {near_rpc_error:?}",
                            transfer_id.origin_chain, transfer_id.origin_nonce
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to finalize Starknet transfer ({:?}:{}): {near_rpc_error:?}",
                            transfer_id.origin_chain,
                            transfer_id.origin_nonce
                        );
                    }
                };
            }

            anyhow::bail!(
                "Failed to finalize Starknet transfer ({:?}:{}): {err:?}",
                transfer_id.origin_chain,
                transfer_id.origin_nonce
            );
        }
    }
}

pub async fn process_fin_transfer_event(
    omni_connector: Arc<OmniConnector>,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Starknet {
        tx_hash,
        transfer_id,
    } = fin_transfer
    else {
        anyhow::bail!("Expected Starknet FinTransfer, got: {fin_transfer:?}");
    };

    info!("Processing Starknet FinTransfer ({:?}): {tx_hash}", ChainKind::Strk);

    if let Err(BridgeSdkError::NearRpcError(NearRpcError::RpcQueryError(
        JsonRpcError::ServerError(JsonRpcServerError::HandlerError(
            RpcQueryError::ContractExecutionError { vm_error, .. },
        )),
    ))) = omni_connector.near_get_transfer_message(transfer_id).await
    {
        // TODO: refactor when enum errors will become available on mainnet
        if vm_error.contains("The transfer does not exist") {
            info!("No fee to claim for Starknet FinTransfer ({transfer_id:?})");
            return Ok(EventAction::Remove);
        }
    }

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    let claim_fee_args = omni_connector::ClaimFeeArgs::ClaimFeeWithMpcProofTx {
        chain_kind: ChainKind::Strk,
        tx_hash: tx_hash.clone(),
        transaction_options: TransactionOptions {
            nonce: Some(nonce),
            wait_until: TxExecutionStatus::Included,
            wait_final_outcome_timeout_sec: None,
        },
    };

    match omni_connector.claim_fee(claim_fee_args).await {
        Ok(near_tx_hash) => {
            info!("Claimed Starknet fee: {near_tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(
                        JsonRpcError::TransportError(_) | JsonRpcError::ServerError(_),
                    )
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to claim Starknet fee, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim Starknet fee: {near_rpc_error:?}");
                    }
                };
            }

            anyhow::bail!("Failed to claim Starknet fee: {err:?}");
        }
    }
}

pub async fn process_deploy_token_event(
    omni_connector: Arc<OmniConnector>,
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Starknet { tx_hash } = deploy_token_event else {
        anyhow::bail!("Expected Starknet DeployToken, got: {deploy_token_event:?}");
    };

    info!("Processing Starknet DeployToken ({:?}): {tx_hash}", ChainKind::Strk);

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithMpcProofTx {
        chain_kind: ChainKind::Strk,
        tx_hash,
        transaction_options: TransactionOptions {
            nonce,
            wait_until: near_primitives::views::TxExecutionStatus::Included,
            wait_final_outcome_timeout_sec: None,
        },
    };

    match omni_connector.bind_token(bind_token_args).await {
        Ok(near_tx_hash) => {
            info!("Bound Starknet token: {near_tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcBroadcastTxAsyncError(_)
                    | NearRpcError::RpcQueryError(
                        JsonRpcError::TransportError(_) | JsonRpcError::ServerError(_),
                    )
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to bind Starknet token, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to bind Starknet token: {near_rpc_error:?}");
                    }
                };
            }

            anyhow::bail!("Failed to bind Starknet token: {err:?}");
        }
    }
}
