use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use borsh::BorshDeserialize;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use omni_connector::OmniConnector;
use omni_types::{
    locker_args::StorageDepositAction, prover_result::ProofKind, ChainKind, OmniAddress, H160,
};

use crate::{config, utils};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub chain_kind: ChainKind,
    pub block_number: u64,
    pub log: Log,
    pub tx_logs: Option<TransactionReceipt>,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();

        let events = match utils::redis::get_events(
            &mut redis_connection,
            utils::redis::EVM_INIT_TRANSFER_EVENTS.to_string(),
        )
        .await
        {
            Some(events) => events,
            None => {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                ))
                .await;
                continue;
            }
        };

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(init_transfer_with_timestamp) =
                serde_json::from_str::<InitTransferWithTimestamp>(&event)
            {
                let handler = handle_init_transfer_event(
                    config.clone(),
                    connector.clone(),
                    jsonrpc_client.clone(),
                    redis_connection.clone(),
                    key.clone(),
                    init_transfer_with_timestamp,
                );
                handlers.push(tokio::spawn(handler));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

async fn handle_init_transfer_event(
    config: config::Config,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    mut redis_connection: redis::aio::MultiplexedConnection,
    key: String,
    init_transfer_with_timestamp: InitTransferWithTimestamp,
) {
    let current_timestamp = chrono::Utc::now().timestamp();

    if current_timestamp
        - init_transfer_with_timestamp
            .last_update_timestamp
            .unwrap_or_default()
        < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
    {
        return;
    }

    let init_log = match init_transfer_with_timestamp
        .log
        .log_decode::<utils::evm::InitTransfer>()
    {
        Ok(init_log) => init_log,
        Err(_) => {
            warn!(
                "Failed to decode log as InitTransfer: {:?}",
                init_transfer_with_timestamp.log
            );
            return;
        }
    };

    info!("Received InitTransfer log");

    let tx_hash = match init_transfer_with_timestamp.log.transaction_hash {
        Some(tx_hash) => tx_hash,
        None => {
            warn!("No transaction hash in log: {:?}", init_log);
            return;
        }
    };

    let recipient = match init_log.inner.recipient.parse::<OmniAddress>() {
        Ok(recipient) => recipient,
        Err(_) => {
            warn!(
                "Failed to parse recipient as OmniAddress: {:?}",
                init_log.inner.recipient
            );
            return;
        }
    };

    if current_timestamp - init_transfer_with_timestamp.creation_timestamp
        > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
    {
        warn!(
            "Removing an old InitTransfer: {:?}",
            init_transfer_with_timestamp
        );
        utils::redis::remove_event(
            &mut redis_connection,
            utils::redis::EVM_INIT_TRANSFER_EVENTS,
            &key,
        )
        .await;
        return;
    }

    // TODO: Use existing API to check if fee is sufficient here

    let vaa = utils::evm::get_vaa(
        connector.clone(),
        init_transfer_with_timestamp.chain_kind,
        init_transfer_with_timestamp.tx_logs.clone(),
        &config,
    )
    .await;

    if vaa.is_none() {
        let light_client_latest_block_number =
            match utils::near::get_eth_light_client_last_block_number(&config, &jsonrpc_client)
                .await
            {
                Ok(block_number) => block_number,
                Err(_) => {
                    warn!("Failed to get eth light client last block number");
                    return;
                }
            };

        if init_transfer_with_timestamp.block_number > light_client_latest_block_number {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            return;
        }
    }

    let topic = match init_transfer_with_timestamp.log.topic0() {
        Some(topic) => topic,
        None => {
            warn!("No topic0 in log: {:?}", init_transfer_with_timestamp.log);
            return;
        }
    };

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let prover_args = match utils::evm::construct_prover_args(
        &config,
        vaa,
        tx_hash,
        H256::from_slice(topic.as_slice()),
        ProofKind::InitTransfer,
    )
    .await
    {
        Some(prover_args) => prover_args,
        None => {
            warn!("Failed to construct prover args");
            return;
        }
    };

    let storage_deposit_actions = match get_storage_deposit_actions(
        &connector,
        &init_log.inner,
        &recipient,
        &init_transfer_with_timestamp,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            warn!("{}", err);
            return;
        }
    };

    let fin_transfer_args = omni_connector::FinTransferArgs::NearFinTransfer {
        chain_kind: init_transfer_with_timestamp.chain_kind,
        storage_deposit_actions,
        prover_args,
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized InitTransfer: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::EVM_INIT_TRANSFER_EVENTS,
                &key,
            )
            .await;
        }
        Err(err) => error!("Failed to finalize InitTransfer: {}", err),
    }
}

async fn get_storage_deposit_actions(
    connector: &OmniConnector,
    init_log: &utils::evm::InitTransfer,
    recipient: &OmniAddress,
    init_transfer_with_timestamp: &InitTransferWithTimestamp,
) -> Result<Vec<StorageDepositAction>, String> {
    let mut storage_deposit_actions = Vec::new();

    if let OmniAddress::Near(near_recipient) = recipient {
        let evm_token_address =
            H160::try_from_slice(init_log.tokenAddress.as_slice()).map_err(|_| {
                format!(
                    "Failed to parse token address as H160: {:?}",
                    init_log.tokenAddress
                )
            })?;

        let omni_token_address = OmniAddress::new_from_evm_address(
            init_transfer_with_timestamp.chain_kind,
            evm_token_address.clone(),
        )
        .map_err(|_| {
            format!(
                "Failed to convert EVM token address to OmniAddress: {:?}",
                evm_token_address
            )
        })?;

        let token_id = connector
            .near_get_token_id(omni_token_address.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get token id by omni token address: {:?}",
                    omni_token_address
                )
            })?;

        let near_recipient_storage_deposit_amount = connector
            .near_get_required_storage_deposit(token_id.clone(), near_recipient.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get required storage deposit for recipient: {:?}",
                    near_recipient
                )
            })?;

        storage_deposit_actions.push(StorageDepositAction {
            token_id,
            account_id: near_recipient.clone(),
            storage_deposit_amount: Some(near_recipient_storage_deposit_amount),
        });
    };

    if init_log.fee > 0 {
        let evm_token_address =
            H160::try_from_slice(init_log.tokenAddress.as_slice()).map_err(|_| {
                format!(
                    "Failed to parse token address as H160: {:?}",
                    init_log.tokenAddress
                )
            })?;

        let omni_token_address = OmniAddress::new_from_evm_address(
            init_transfer_with_timestamp.chain_kind,
            evm_token_address.clone(),
        )
        .map_err(|_| {
            format!(
                "Failed to convert EVM token address to OmniAddress: {:?}",
                evm_token_address
            )
        })?;

        let token_id = connector
            .near_get_token_id(omni_token_address.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get token id by omni token address: {:?}",
                    omni_token_address
                )
            })?;

        let relayer = connector
            .near_bridge_client()
            .and_then(|client| client.signer().map(|signer| signer.account_id))
            .map_err(|_| "Failed to get relayer account id".to_string())?;

        let near_relayer_storage_deposit_amount = connector
            .near_get_required_storage_deposit(token_id.clone(), relayer.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get required storage deposit for recipient: {:?}",
                    relayer
                )
            })?;

        storage_deposit_actions.push(StorageDepositAction {
            token_id,
            account_id: relayer,
            storage_deposit_amount: Some(near_relayer_storage_deposit_amount),
        });
    }

    if init_log.nativeFee > 0 {
        let omni_token_address = OmniAddress::new_zero(init_transfer_with_timestamp.chain_kind)
            .map_err(|_| {
                format!(
                    "Failed to get zero EVM address for chain: {:?}",
                    init_transfer_with_timestamp.chain_kind
                )
            })?;

        let token_id = connector
            .near_get_token_id(omni_token_address.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get token id by omni token address: {:?}",
                    omni_token_address
                )
            })?;

        let relayer = connector
            .near_bridge_client()
            .and_then(|client| client.signer().map(|signer| signer.account_id))
            .map_err(|_| "Failed to get relayer account id".to_string())?;

        storage_deposit_actions.push(StorageDepositAction {
            token_id,
            account_id: relayer,
            storage_deposit_amount: None,
        });
    }

    Ok(storage_deposit_actions)
}
