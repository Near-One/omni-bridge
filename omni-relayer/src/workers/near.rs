use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use log::{info, warn};

use near_bridge_client::{FastFinTransferArgs, TransactionOptions};
use near_jsonrpc_client::{JsonRpcClient, errors::JsonRpcError};
use near_primitives::{hash::CryptoHash, types::AccountId};
use near_rpc_client::NearRpcError;
use solana_client::rpc_request::RpcResponseErrorData;
use solana_rpc_client_api::{client_error::ErrorKind, request::RpcError};
use solana_sdk::{instruction::InstructionError, pubkey::Pubkey, transaction::TransactionError};

use omni_connector::OmniConnector;
use omni_types::{ChainKind, FastTransfer, OmniAddress, TransferId, near_events::OmniBridgeEvent};

use crate::{config, utils, workers::PAUSED_ERROR};

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
    config: config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    connector: Arc<OmniConnector>,
    signer: AccountId,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Near {
        ref transfer_message,
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

    info!("Trying to process TransferMessage on NEAR");

    match connector
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
        match utils::bridge_api::is_fee_sufficient(
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
                warn!("Insufficient fee for transfer: {transfer_message:?}");
                return Ok(EventAction::Retry);
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {err:?}");
                return Ok(EventAction::Retry);
            }
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
                wait_final_outcome_timeout_sec: None,
            },
        )
        .await
    {
        Ok(tx_hash) => {
            utils::redis::add_event(
                redis_connection,
                utils::redis::EVENTS,
                tx_hash.to_string(),
                UnverifiedTrasfer {
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

            info!("Signed transfer: {tx_hash:?}");

            Ok(EventAction::Remove)
        }
        Err(err) => {
            if current_timestamp - creation_timestamp
                > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
            {
                anyhow::bail!("Transfer ({}) is too old", transfer_message.origin_nonce);
            }

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
    config: config::Config,
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

    info!("Trying to process SignTransferEvent log on NEAR");

    match connector
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

    if message_payload.fee_recipient != Some(signer) {
        anyhow::bail!("Fee recipient mismatch");
    }

    if config.is_bridge_api_enabled() {
        let transfer_message = match connector
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

        match utils::bridge_api::is_fee_sufficient(
            &config,
            transfer_message.fee,
            &transfer_message.sender,
            &transfer_message.recipient,
            &transfer_message.token,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                warn!("Insufficient fee for transfer: {message_payload:?}");
                return Ok(EventAction::Retry);
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {err:?}");
                return Ok(EventAction::Retry);
            }
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
    redis_connection: &mut redis::aio::MultiplexedConnection,
    jsonrpc_client: JsonRpcClient,
    unverified_event: UnverifiedTrasfer,
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

pub async fn process_fast_transfer_event(
    connector: Arc<OmniConnector>,
    near_fast_bridge_client: Arc<near_bridge_client::NearBridgeClient>,
    tx_hash: &str,
    transfer: Transfer,
    near_fast_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Fast {
        block_number,
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

    let Ok(token_id) = utils::storage::get_token_id(
        &near_fast_bridge_client.clone(),
        transfer_id.origin_chain,
        &token,
    )
    .await
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

    match connector
        .near_is_fast_transfer_finalised(fast_transfer.id())
        .await
    {
        Ok(true) => anyhow::bail!("Fast transfer is already finalised: {:?}", transfer),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if fast transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    match connector.near_is_transfer_finalised(transfer_id).await {
        Ok(true) => anyhow::bail!("Transfer is already finalised: {:?}", transfer),
        Ok(false) => {}
        Err(err) => {
            warn!("Failed to check if transfer is finalised: {err:?}");
            return Ok(EventAction::Retry);
        }
    }

    let Ok(tx_hash) = tx_hash.parse() else {
        anyhow::bail!("Failed to parse tx_hash: {tx_hash}");
    };

    if let Err(err) = connector
        .evm_get_transfer_event(transfer_id.origin_chain, tx_hash)
        .await
    {
        warn!("Failed to get transfer event for tx_hash {tx_hash}: {err:?}");
        return Ok(EventAction::Retry);
    }

    let Ok(last_finalized_block_number) = connector
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

    let relayer = near_fast_bridge_client
        .account_id()
        .context("Failed to get relayer account id")?;

    let Ok(token_omni_address) =
        utils::evm::string_to_evm_omniaddress(transfer_id.origin_chain, &token)
    else {
        anyhow::bail!("Failed to convert token address to OmniAddress: {token}");
    };

    let Some(amount) = amount.0.checked_sub(fee.fee.0) else {
        anyhow::bail!("Amount ({amount:?}) is less than fee ({fee:?}) for token: {token_id}");
    };

    let Ok(amount) = near_fast_bridge_client
        .denormalize_amount(token_omni_address, amount)
        .await
    else {
        warn!("Failed to denormalize amount for token: {token_id}");
        return Ok(EventAction::Retry);
    };

    let Ok(balance) = near_fast_bridge_client
        .ft_balance_of(token_id.clone(), relayer.clone())
        .await
    else {
        warn!("Failed to get balance of relayer: {relayer} for token: {token_id}");
        return Ok(EventAction::Retry);
    };

    if balance < amount {
        anyhow::bail!(
            "Insufficient balance for relayer to perform fast transfer: {relayer} for token: {token_id}"
        );
    }

    let fast_fin_transfer_args = FastFinTransferArgs {
        token_id,
        amount,
        transfer_id,
        recipient,
        fee,
        msg: msg.clone(),
        storage_deposit_amount: storage_deposit_amount.map(|amount| amount.0),
        relayer,
    };

    let nonce = Some(
        near_fast_nonce
            .reserve_nonce()
            .await
            .context("Failed to reserve nonce for near transaction")?,
    );

    match near_fast_bridge_client
        .fast_fin_transfer(
            fast_fin_transfer_args,
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
