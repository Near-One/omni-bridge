use std::sync::Arc;

use anyhow::Result;
use bridge_connector_common::result::BridgeSdkError;
use log::{info, warn};

use near_bridge_client::{
    TransactionOptions,
    btc_connector::{DepositMsg, PostAction},
};
use near_jsonrpc_client::errors::JsonRpcError;
use near_primitives::types::AccountId;
use near_rpc_client::NearRpcError;

use omni_connector::{BtcDepositArgs, OmniConnector};

use crate::utils;

use super::{EventAction, Transfer};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SignBtcTransaction {
    pub near_tx_hash: String,
    pub relayer: AccountId,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ConfirmedTxHash {
    pub btc_tx_hash: String,
}

pub async fn process_near_to_btc_init_transfer_event(
    connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::NearToBtc {
        btc_pending_id,
        sign_index,
    } = transfer
    else {
        anyhow::bail!("Expected NearToBtcTransfer, got: {:?}", transfer);
    };

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match connector
        .near_sign_btc_transaction(
            btc_pending_id,
            sign_index,
            TransactionOptions {
                nonce,
                wait_until: near_primitives::views::TxExecutionStatus::Included,
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Signed BTC transaction: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to sign BTC transaction, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to sign BTC transaction: {near_rpc_error:?}");
                    }
                };
            }
            anyhow::bail!("Failed to sign BTC transaction: {err:?}");
        }
    }
}

pub async fn process_btc_to_near_init_transfer_event(
    connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::BtcToNear {
        btc_tx_hash,
        vout,
        deposit_msg,
    } = transfer
    else {
        anyhow::bail!("Expected BtcToNearTransfer, got: {:?}", transfer);
    };

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match connector
        .near_fin_transfer_btc(
            btc_tx_hash,
            usize::try_from(vout)?,
            BtcDepositArgs::DepositMsg {
                msg: DepositMsg {
                    recipient_id: deposit_msg.recipient_id,
                    post_actions: deposit_msg.post_actions.map(|optional_actions| {
                        optional_actions
                            .into_iter()
                            .map(|action| PostAction {
                                receiver_id: action.receiver_id,
                                amount: action.amount.0,
                                memo: action.memo,
                                msg: action.msg,
                                gas: action.gas,
                            })
                            .collect()
                    }),
                    extra_msg: deposit_msg.extra_msg,
                },
            },
            TransactionOptions {
                nonce,
                wait_until: near_primitives::views::TxExecutionStatus::Included,
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Finalized BTC transaction: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to finalize BTC transaction, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to finalize BTC transaction: {near_rpc_error:?}");
                    }
                };
            }
            anyhow::bail!("Failed to finalize BTC transaction: {err:?}");
        }
    }
}

pub async fn process_sign_transaction_event(
    connector: Arc<OmniConnector>,
    sign_btc_transaction_event: SignBtcTransaction,
) -> Result<EventAction> {
    info!("Trying to process SignBtcTransaction log on NEAR");

    match connector
        .btc_fin_transfer(
            sign_btc_transaction_event.near_tx_hash.clone(),
            Some(sign_btc_transaction_event.relayer),
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Finalized BTC transaction: {tx_hash}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!(
                            "Failed to finalize btc transaction ({}), retrying: {near_rpc_error:?}",
                            sign_btc_transaction_event.near_tx_hash
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to finalize btc transaction ({}): {near_rpc_error:?}",
                            sign_btc_transaction_event.near_tx_hash
                        );
                    }
                };
            }
            anyhow::bail!(
                "Failed to finalize btc transaction ({}): {err:?}",
                sign_btc_transaction_event.near_tx_hash
            );
        }
    }
}

pub async fn process_confirmed_tx_hash(
    connector: Arc<OmniConnector>,
    btc_tx_hash: String,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match connector
        .near_btc_verify_withdraw(
            btc_tx_hash,
            TransactionOptions {
                nonce,
                wait_until: near_primitives::views::TxExecutionStatus::Included,
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Verified withdraw: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to verify withdraw, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to verify withdraw: {near_rpc_error:?}");
                    }
                };
            }

            anyhow::bail!("Failed to verify withdraw: {err:?}");
        }
    }
}
