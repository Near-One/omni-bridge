use std::sync::Arc;

use anyhow::Result;
use bridge_connector_common::result::BridgeSdkError;
use log::{info, warn};

use near_bridge_client::TransactionOptions;
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
pub struct ConfirmedTxid {
    pub txid: String,
}

pub async fn process_init_transfer_event(
    connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Btc {
        tx_hash,
        vout,
        recipient_id,
        ..
    } = transfer
    else {
        anyhow::bail!("Expected BtcTransfer, got: {:?}", transfer);
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
            tx_hash,
            usize::try_from(vout)?,
            BtcDepositArgs::OmniDepositArgs {
                recipient_id,
                amount: 0,
                fee: 0,
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

pub async fn process_confirmed_txid(
    connector: Arc<OmniConnector>,
    txid: String,
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
            txid,
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
