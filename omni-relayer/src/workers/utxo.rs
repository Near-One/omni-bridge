use std::{str::FromStr, sync::Arc};

use anyhow::Result;
use bridge_connector_common::result::BridgeSdkError;
use near_bridge_client::{
    TransactionOptions,
    btc::{DepositMsg, PostAction, SafeDepositMsg},
};
use near_jsonrpc_client::errors::JsonRpcError;
use near_primitives::{hash::CryptoHash, types::AccountId};
use near_rpc_client::NearRpcError;
use omni_types::ChainKind;
use tracing::{info, warn};

use omni_connector::{BtcDepositArgs, FinTransferArgs, OmniConnector};

use crate::{
    config, utils,
    workers::{RetryableEvent, near::UnverifiedTrasfer},
};

use super::{EventAction, Transfer};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SignUtxoTransaction {
    pub chain: ChainKind,
    pub near_tx_hash: String,
    pub relayer: AccountId,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ConfirmedTxHash {
    pub chain: ChainKind,
    pub btc_tx_hash: String,
}

pub async fn process_near_to_utxo_init_transfer_event(
    omni_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::NearToUtxo {
        chain,
        btc_pending_id,
        sign_index,
    } = transfer
    else {
        anyhow::bail!("Expected NearToUtxoTransfer, got: {transfer:?}");
    };

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match omni_connector
        .near_sign_btc_transaction(
            chain,
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
            info!("Signed {chain:?} transaction: {tx_hash:?}");
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
                    | NearRpcError::RpcTransactionError(_) => {
                        warn!("Failed to sign {chain:?} transaction, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to sign {chain:?} transaction: {near_rpc_error:?}");
                    }
                };
            }
            anyhow::bail!("Failed to sign {chain:?} transaction: {err:?}");
        }
    }
}

pub async fn process_utxo_to_near_init_transfer_event(
    omni_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Ok(near_bridge_client) = omni_connector.near_bridge_client() else {
        anyhow::bail!("Near bridge client is not configured");
    };

    let Transfer::UtxoToNear {
        chain,
        btc_tx_hash,
        vout,
        deposit_msg,
    } = transfer
    else {
        anyhow::bail!("Expected UtxoToNearTransfer, got: {transfer:?}");
    };

    let mut nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match omni_connector
        .near_get_required_storage_deposit(
            near_bridge_client.utxo_chain_token(chain)?,
            deposit_msg.recipient_id.clone(),
        )
        .await
    {
        Ok(amount) if amount > 0 => {
            if omni_connector
                .near_storage_deposit_for_token(
                    near_bridge_client.utxo_chain_token(chain)?,
                    amount,
                    deposit_msg.recipient_id.clone(),
                    TransactionOptions {
                        nonce,
                        wait_until: near_primitives::views::TxExecutionStatus::Final,
                        wait_final_outcome_timeout_sec: None,
                    },
                )
                .await
                .is_err()
            {
                warn!(
                    "Failed to deposit storage for token {:?} to {}",
                    near_bridge_client.utxo_chain_token(chain)?,
                    deposit_msg.recipient_id
                );
                return Ok(EventAction::Retry);
            }

            nonce = match near_nonce.reserve_nonce().await {
                Ok(nonce) => Some(nonce),
                Err(err) => {
                    warn!("Failed to reserve nonce: {err:?}");
                    return Ok(EventAction::Retry);
                }
            };
        }
        Ok(_) => {}
        Err(err) => {
            warn!(
                "Failed to get required storage deposit for token {:?} to {}: {err:?}",
                near_bridge_client.utxo_chain_token(chain)?,
                deposit_msg.recipient_id
            );
            return Ok(EventAction::Retry);
        }
    }

    let fin_transfer_args = FinTransferArgs::NearFinTransferBTC {
        chain_kind: chain,
        btc_tx_hash: btc_tx_hash.clone(),
        vout: usize::try_from(vout)?,
        btc_deposit_args: BtcDepositArgs::DepositMsg {
            msg: DepositMsg {
                recipient_id: deposit_msg.recipient_id.clone(),
                post_actions: deposit_msg.post_actions.map(|optional_actions| {
                    optional_actions
                        .into_iter()
                        .map(|action| PostAction {
                            receiver_id: action.receiver_id,
                            amount: action.amount.0,
                            memo: action.memo,
                            msg: action.msg,
                            gas: action
                                .gas
                                .map(|gas| near_primitives::gas::Gas::from_gas(gas.as_gas())),
                        })
                        .collect()
                }),
                extra_msg: deposit_msg.extra_msg,
                safe_deposit: deposit_msg.safe_deposit.map(|safe_deposit| SafeDepositMsg {
                    msg: safe_deposit.msg,
                }),
            },
        },
        transaction_options: TransactionOptions {
            nonce,
            wait_until: near_primitives::views::TxExecutionStatus::Included,
            wait_final_outcome_timeout_sec: None,
        },
    };

    match omni_connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized {chain:?} transaction: {tx_hash:?}");
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
                    | NearRpcError::RpcTransactionError(_) => {
                        warn!(
                            "Failed to finalize {chain:?} transaction, retrying: {near_rpc_error:?}"
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to finalize {chain:?} transaction: {near_rpc_error:?}"
                        );
                    }
                };
            }

            if let BridgeSdkError::LightClientNotSynced(block) = err {
                warn!(
                    "{chain:?} light client is not synced yet for transfer ({btc_tx_hash}), block: {block}",
                );
                return Ok(EventAction::Retry);
            }

            anyhow::bail!("Failed to finalize {chain:?} transaction: {err:?}");
        }
    }
}

pub async fn process_sign_transaction_event(
    omni_connector: Arc<OmniConnector>,
    sign_utxo_transaction_event: SignUtxoTransaction,
) -> Result<EventAction> {
    info!("Trying to process SignBtcTransaction log on NEAR");

    let Ok(near_tx_hash) = CryptoHash::from_str(&sign_utxo_transaction_event.near_tx_hash) else {
        anyhow::bail!(
            "Invalid near tx hash: {}",
            sign_utxo_transaction_event.near_tx_hash
        );
    };

    match omni_connector
        .btc_fin_transfer(
            sign_utxo_transaction_event.chain,
            near_tx_hash,
            Some(sign_utxo_transaction_event.relayer),
        )
        .await
    {
        Ok(tx_hash) => {
            info!(
                "Finalized {:?} transaction: {tx_hash}",
                sign_utxo_transaction_event.chain
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
                    | NearRpcError::RpcTransactionError(_) => {
                        warn!(
                            "Failed to finalize {:?} transaction ({}), retrying: {near_rpc_error:?}",
                            sign_utxo_transaction_event.chain,
                            sign_utxo_transaction_event.near_tx_hash
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to finalize {:?} transaction ({}): {near_rpc_error:?}",
                            sign_utxo_transaction_event.chain,
                            sign_utxo_transaction_event.near_tx_hash
                        );
                    }
                };
            } else if let BridgeSdkError::UtxoRpcError(err) = err {
                warn!(
                    "Failed to finalize {:?} transaction ({}), retrying: {err:?}",
                    sign_utxo_transaction_event.chain, sign_utxo_transaction_event.near_tx_hash
                );
                return Ok(EventAction::Retry);
            }

            anyhow::bail!(
                "Failed to finalize {:?} transaction ({}): {err:?}",
                sign_utxo_transaction_event.chain,
                sign_utxo_transaction_event.near_tx_hash
            );
        }
    }
}

pub async fn process_confirmed_tx_hash(
    config: &config::Config,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
    key: String,
    omni_connector: Arc<OmniConnector>,
    signer: AccountId,
    confirmed_tx_hash: ConfirmedTxHash,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    match omni_connector
        .near_btc_verify_withdraw(
            confirmed_tx_hash.chain,
            confirmed_tx_hash.btc_tx_hash.clone(),
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

            let Ok(serialized_event) = serde_json::to_value(&confirmed_tx_hash) else {
                warn!("Failed to serialize confirmed tx: {confirmed_tx_hash:?}");
                return Ok(EventAction::Remove);
            };

            utils::redis::add_event(
                config,
                redis_connection_manager,
                utils::redis::EVENTS,
                tx_hash.to_string(),
                RetryableEvent::new(UnverifiedTrasfer {
                    tx_hash,
                    signer,
                    specific_errors: Some(vec!["Not enough blocks confirmed".to_string()]),
                    original_key: key,
                    original_event: serialized_event,
                }),
            )
            .await;

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
                    | NearRpcError::RpcTransactionError(_) => {
                        warn!("Failed to verify withdraw, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to verify withdraw: {near_rpc_error:?}");
                    }
                };
            }

            if let BridgeSdkError::LightClientNotSynced(block) = err {
                warn!(
                    "Light client is not synced yet for {}, block: {block}",
                    confirmed_tx_hash.btc_tx_hash
                );
                return Ok(EventAction::Retry);
            }

            anyhow::bail!("Failed to verify withdraw: {err:?}");
        }
    }
}
