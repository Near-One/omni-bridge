use std::sync::Arc;

use anyhow::{Context, Result};
use bridge_connector_common::result::BridgeSdkError;
use tracing::{info, warn};

use near_bridge_client::{NearBridgeClient, TransactionOptions};
use near_jsonrpc_client::errors::JsonRpcError;
use near_primitives::views::TxExecutionStatus;
use near_rpc_client::NearRpcError;

use omni_connector::OmniConnector;
use omni_types::{
    ChainKind, Fee, OmniAddress, TransferId, locker_args::ClaimFeeArgs,
    prover_args::WormholeVerifyProofArgs, prover_result::ProofKind,
};

use crate::{config, utils};

use super::{DeployToken, EventAction, FinTransfer, Transfer};

pub async fn process_init_transfer_event(
    config: &config::Config,
    redis_connection: &mut redis::aio::MultiplexedConnection,
    key: String,
    omni_connector: Arc<OmniConnector>,
    transfer: Transfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let Transfer::Solana {
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
        < config.redis.check_insufficient_fee_transfers_every_secs
    {
        return Ok(EventAction::Retry);
    }

    info!("Trying to process InitTransfer log on Solana");

    let transfer_id = TransferId {
        origin_chain: sender.get_chain(),
        origin_nonce: sequence,
    };

    match omni_connector
        .is_transfer_finalised(Some(sender.get_chain()), recipient.get_chain(), sequence)
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
        let token =
            OmniAddress::new_from_slice(ChainKind::Sol, &token.to_bytes()).map_err(|err| {
                anyhow::anyhow!("Failed to parse \"{}\" as `OmniAddress`: {:?}", sender, err)
            })?;

        let Ok(needed_fee) =
            utils::bridge_api::TransferFee::get_transfer_fee(&config, sender, recipient, &token)
                .await
        else {
            warn!("Failed to get transfer fee for transfer: {transfer:?}");
            return Ok(EventAction::Retry);
        };

        let provided_fee = Fee {
            fee,
            native_fee: u128::from(native_fee).into(),
        };

        if let Some(event_action) = needed_fee
            .check_fee(
                &config,
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

    let Ok(vaa) = omni_connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, &emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {sequence}");
        return Ok(EventAction::Retry);
    };

    let fee_recipient = omni_connector
        .near_bridge_client()
        .and_then(NearBridgeClient::account_id)
        .context("Failed to get relayer account id")?;

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &omni_connector,
        ChainKind::Sol,
        recipient,
        &fee_recipient,
        &token.to_string(),
        fee.0,
        u128::from(native_fee),
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            utils::redis::add_event(
                config,
                redis_connection,
                utils::redis::STUCK_EVENTS,
                &key,
                transfer,
            )
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
            wait_final_outcome_timeout_sec: None,
        },
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
                        warn!("Failed to finalize transfer, retrying: {near_rpc_error:?}",);
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to finalize transfer: {near_rpc_error:?}");
                    }
                };
            }

            anyhow::bail!("Failed to finalize transfer: {err:?}");
        }
    }
}

pub async fn process_fin_transfer_event(
    config: &config::Config,
    omni_connector: Arc<OmniConnector>,
    fin_transfer: FinTransfer,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let FinTransfer::Solana { emitter, sequence } = fin_transfer else {
        anyhow::bail!("Expected Solana FinTransfer, got: {:?}", fin_transfer);
    };

    info!("Trying to process FinTransfer log on Solana");

    let Ok(vaa) = omni_connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {sequence}");
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
            if let BridgeSdkError::NearRpcError(ref near_rpc_error) = err {
                match near_rpc_error {
                    NearRpcError::NonceError
                    | NearRpcError::FinalizationError
                    | NearRpcError::RpcTransactionError(JsonRpcError::TransportError(_)) => {
                        warn!("Failed to claim fee, retrying: {near_rpc_error:?}");
                        return Ok(EventAction::Retry);
                    }
                    _ => {
                        anyhow::bail!("Failed to claim fee: {err:?}");
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
    deploy_token_event: DeployToken,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<EventAction> {
    let DeployToken::Solana { emitter, sequence } = deploy_token_event else {
        anyhow::bail!("Expected Solana DeployToken, got: {:?}", deploy_token_event);
    };

    info!("Trying to process DeployToken log on Solana");

    let Ok(vaa) = omni_connector
        .wormhole_get_vaa(config.wormhole.solana_chain_id, emitter, sequence)
        .await
    else {
        warn!("Failed to get VAA for sequence: {sequence}");
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
            warn!("Failed to reserve nonce: {err:?}");
            return Ok(EventAction::Retry);
        }
    };

    let bind_token_args = omni_connector::BindTokenArgs::BindTokenWithArgs {
        chain_kind: ChainKind::Sol,
        prover_args,
        transaction_options: TransactionOptions {
            nonce,
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
