use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use tracing::{info, warn};

use near_bridge_client::TransactionOptions;
use near_jsonrpc_client::{JsonRpcClient, errors::JsonRpcError};
use near_primitives::{hash::CryptoHash, types::AccountId};
use near_rpc_client::NearRpcError;
use solana_client::rpc_request::RpcResponseErrorData;
use solana_rpc_client_api::{client_error::ErrorKind, request::RpcError};
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

use omni_connector::OmniConnector;
use omni_types::{ChainKind, FastTransfer, OmniAddress, TransferId, near_events::OmniBridgeEvent};

use crate::{
    config, utils,
    workers::{PAUSED_ERROR, RetryableEvent},
};

use super::{EventAction, Transfer};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UnverifiedTrasfer {
    tx_hash: CryptoHash,
    signer: AccountId,
    specific_errors: Option<Vec<String>>,
    original_key: String,
    original_event: Transfer,
}

pub async fn process_transfer_event(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    omni_connector: Arc<OmniConnector>,
    signer: AccountId,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Near {
        ref transfer_message,
    } = transfer
    else {
        anyhow::bail!("Expected NearTransferWithTimestamp, got: {:?}", transfer);
    };

    info!("Trying to process TransferMessage on NEAR");

    match omni_connector
        .is_transfer_finalised(
            Some(transfer_message.get_origin_chain()),
            transfer_message.get_destination_chain(),
            transfer_message.destination_nonce,
        )
        .await
    {
        Ok(true) => anyhow::bail!("Transfer is already finalised: {:?}", transfer_message),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    if config.is_bridge_api_enabled()
        && !config
            .near
            .sign_without_checking_fee
            .as_ref()
            .is_some_and(|list| list.contains(&transfer_message.sender))
    {
        let Ok(needed_fee) = utils::bridge_api::TransferFee::get_transfer_fee(
            config,
            &transfer_message.sender,
            &transfer_message.recipient,
            &transfer_message.token,
        )
        .await
        else {
            warn!("Failed to get transfer fee for transfer: {transfer_message:?}");
            return Ok(EventAction::Retry);
        };

        if let Some(event_action) = needed_fee
            .check_fee(
                config,
                redis_connection,
                &transfer_message,
                transfer_message.get_transfer_id(),
                &transfer_message.fee,
            )
            .await
        {
            return Ok(event_action);
        }
    }

    let nonce = near_nonce
        .reserve_nonce()
        .await
        .context("Failed to reserve nonce for near transaction")?;

    match omni_connector
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
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            utils::redis::add_event(
                config,
                redis_connection,
                utils::redis::EVENTS,
                tx_hash.to_string(),
                RetryableEvent::new(UnverifiedTrasfer {
                    tx_hash,
                    signer,
                    specific_errors: Some(vec![
                        "Signature request has already been submitted. Please try again later."
                            .to_string(),
                        "Signature request has timed out.".to_string(),
                        "Attached deposit is lower than required".to_string(),
                        "Exceeded the prepaid gas.".to_string(),
                    ]),
                    original_key: key,
                    original_event: transfer,
                }),
            )
            .await;

            info!("Signed transfer: {tx_hash:?}");

            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!(
                            "Failed to sign transfer ({}), retrying: {near_rpc_error:?}",
                            transfer_message.origin_nonce
                        );
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!(
                            "Failed to sign transfer ({}): {near_rpc_error:?}",
                            transfer_message.origin_nonce
                        );
                    }
                };
            }
            anyhow::bail!(
                "Failed to sign transfer ({}): {err:?}",
                transfer_message.origin_nonce
            );
        }
    }
}

pub async fn process_sign_transfer_event(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    omni_connector: Arc<OmniConnector>,
    signer: AccountId,
    omni_bridge_event: OmniBridgeEvent,
    evm_nonces: Arc<utils::nonce::EvmNonceManagers>,
) -> Result<EventAction> {
    let OmniBridgeEvent::SignTransferEvent {
        message_payload, ..
    } = &omni_bridge_event
    else {
        anyhow::bail!("Expected SignTransferEvent, got: {:?}", omni_bridge_event);
    };

    info!("Trying to process SignTransferEvent log on NEAR");

    if message_payload.fee_recipient != Some(signer) {
        anyhow::bail!("Fee recipient mismatch");
    }

    match omni_connector
        .is_transfer_finalised(
            None,
            message_payload.recipient.get_chain(),
            message_payload.destination_nonce,
        )
        .await
    {
        Ok(true) => anyhow::bail!(
            "Transfer is already finalised: {:?}",
            message_payload.transfer_id
        ),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    if config.is_bridge_api_enabled() {
        let transfer_message = match omni_connector
            .near_get_transfer_message(message_payload.transfer_id)
            .await
        {
            Ok(transfer_message) => transfer_message,
            Err(err) => {
                if err.to_string().contains("The transfer does not exist") {
                    anyhow::bail!(
                        "Transfer does not exist: {:?} (probably fee is 0 or transfer was already finalized)",
                        message_payload.transfer_id
                    );
                }

                warn!(
                    "Failed to get transfer message: {:?}",
                    message_payload.transfer_id
                );

                return Ok(EventAction::Retry);
            }
        };

        let Ok(needed_fee) = utils::bridge_api::TransferFee::get_transfer_fee(
            config,
            &transfer_message.sender,
            &transfer_message.recipient,
            &transfer_message.token,
        )
        .await
        else {
            warn!("Failed to get transfer fee for transfer: {transfer_message:?}");
            return Ok(EventAction::Retry);
        };

        if let Some(event_action) = needed_fee
            .check_fee(
                config,
                redis_connection,
                &transfer_message,
                transfer_message.get_transfer_id(),
                &transfer_message.fee,
            )
            .await
        {
            return Ok(event_action);
        }
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
                event: omni_bridge_event,
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
                event: omni_bridge_event,
                solana_token: Pubkey::new_from_array(token.0),
            }
        }
    };

    match omni_connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized deposit: {tx_hash}");
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

pub async fn process_unverified_transfer_event(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: JsonRpcClient,
    unverified_event: UnverifiedTrasfer,
) {
    utils::redis::remove_event(
        config,
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
            config,
            redis_connection,
            utils::redis::EVENTS,
            unverified_event.original_key,
            RetryableEvent::new(unverified_event.original_event),
        )
        .await;
    }
}

pub async fn initiate_fast_transfer(
    fast_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_omni_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Fast {
        block_number,
        tx_hash,
        token,
        amount,
        transfer_id,
        recipient,
        fee,
        msg,
        storage_deposit_amount,
        safe_confirmations,
    } = transfer.clone()
    else {
        anyhow::bail!("Expected FastTransferEvent, got: {:?}", transfer);
    };

    // TODO: Fast transfer to other chain increases origin nonce by one, so regular relayer won't
    // be able to finalize it with a normal sign transfer. We need to catch and sign
    // `FastTransferEvent`. This will be possible once bridge-indexer will track these events
    // Related PR: https://github.com/Near-One/bridge-indexer-rs/pull/195
    if recipient.get_chain() != ChainKind::Near {
        anyhow::bail!(
            "Fast transfer is supported only for transfers to NEAR for now, got: {:?}",
            recipient.get_chain()
        );
    }

    info!("Trying to initiate FastTransfer on NEAR");

    let Ok(token_id) =
        utils::storage::get_token_id(&fast_connector, transfer_id.origin_chain, &token).await
    else {
        warn!("Failed to get token id for transfer: {transfer_id:?}");
        return Ok(EventAction::Retry);
    };

    let fast_transfer = FastTransfer {
        transfer_id,
        token_id: token_id.clone(),
        amount,
        fee: fee.clone(),
        recipient: recipient.clone(),
        msg: msg.clone(),
    };

    match fast_connector.near_is_transfer_finalised(transfer_id).await {
        Ok(true) => anyhow::bail!("Transfer is already finalised: {:?}", transfer),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    match fast_connector
        .near_get_fast_transfer_status(fast_transfer.id())
        .await
    {
        Ok(Some(_)) => anyhow::bail!("Fast transfer is already finalised: {:?}", transfer),
        Ok(None) => {}
        Err(err) => {
            warn!("Failed to check if fast transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    let Ok(last_finalized_block_number) = fast_connector
        .evm_get_last_block_number(transfer_id.origin_chain)
        .await
    else {
        warn!("Failed to get last finalized block number for EVM chain");
        return Ok(EventAction::Retry);
    };

    let current_confirmations = last_finalized_block_number.saturating_sub(block_number);

    if current_confirmations < safe_confirmations {
        warn!(
            "Fast transfer block number ({block_number}) is not finalized yet, waiting for more confirmations. Current confirmations: {current_confirmations}",
        );
        return Ok(EventAction::Retry);
    }

    let nonce = Some(
        near_omni_nonce
            .reserve_nonce()
            .await
            .context("Failed to reserve nonce for near transaction")?,
    );

    match fast_connector
        .near_fast_transfer(
            transfer_id.origin_chain,
            tx_hash,
            storage_deposit_amount.map(|storage_deposit_amount| storage_deposit_amount.0),
            TransactionOptions {
                nonce,
                wait_until: near_primitives::views::TxExecutionStatus::Included,
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            info!("Fast transfer initiated successfully: {tx_hash:?}");
            Ok(EventAction::Remove)
        }
        Err(err) => {
            if let BridgeSdkError::NearRpcError(near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to initiate fast transfer, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to initiate fast transfer: {near_rpc_error:?}");
                    }
                };
            }
            anyhow::bail!("Failed to initiate fast transfer: {err:?}");
        }
    }
}
