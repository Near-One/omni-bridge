use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{info, warn};

use alloy::rpc::types::{Log, TransactionReceipt};
use ethereum_types::H256;
use omni_connector::OmniConnector;
#[cfg(not(feature = "disable_fee_check"))]
use omni_types::Fee;
use omni_types::{ChainKind, OmniAddress};

use crate::{config, utils};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub chain_kind: ChainKind,
    pub block_number: u64,
    pub log: Log,
    pub tx_logs: Option<Box<TransactionReceipt>>,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
    pub expected_finalization_time: i64,
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

        let Some(events) = utils::redis::get_events(
            &mut redis_connection,
            utils::redis::EVM_INIT_TRANSFER_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
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
        < init_transfer_with_timestamp.creation_timestamp
            + init_transfer_with_timestamp.expected_finalization_time
    {
        return;
    }

    if current_timestamp
        - init_transfer_with_timestamp
            .last_update_timestamp
            .unwrap_or_default()
        < utils::redis::CHECK_INSUFFICIENT_FEE_TRANSFERS_EVERY_SECS
    {
        return;
    }

    let Ok(init_log) = init_transfer_with_timestamp
        .log
        .log_decode::<utils::evm::InitTransfer>()
    else {
        warn!(
            "Failed to decode log as InitTransfer: {:?}",
            init_transfer_with_timestamp.log
        );
        return;
    };

    info!(
        "Trying to process InitTransfer log on {:?}",
        init_transfer_with_timestamp.chain_kind
    );

    let Some(tx_hash) = init_transfer_with_timestamp.log.transaction_hash else {
        warn!("No transaction hash in log: {:?}", init_log);
        return;
    };

    let Ok(recipient) = init_log.inner.recipient.parse::<OmniAddress>() else {
        warn!(
            "Failed to parse recipient as OmniAddress: {:?}",
            init_log.inner.recipient
        );
        return;
    };

    #[cfg(not(feature = "disable_fee_check"))]
    {
        let sender = match utils::evm::string_to_evm_omniaddress(
            init_transfer_with_timestamp.chain_kind,
            &init_log.inner.sender.to_string(),
        ) {
            Ok(sender) => sender,
            Err(err) => {
                warn!("{}", err);
                return;
            }
        };

        let token = match utils::evm::string_to_evm_omniaddress(
            init_transfer_with_timestamp.chain_kind,
            &init_log.inner.tokenAddress.to_string(),
        ) {
            Ok(token) => token,
            Err(err) => {
                warn!("{}", err);
                return;
            }
        };

        match utils::fee::is_fee_sufficient(
            &config,
            Fee {
                fee: init_log.inner.fee.into(),
                native_fee: init_log.inner.nativeFee.into(),
            },
            &sender,
            &recipient,
            &token,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                warn!(
                    "Insufficient fee for transfer: {:?}",
                    init_transfer_with_timestamp
                );
                return;
            }
            Err(err) => {
                warn!("Failed to check fee sufficiency: {}", err);
                return;
            }
        }
    }

    let vaa = utils::evm::get_vaa_from_evm_log(
        connector.clone(),
        init_transfer_with_timestamp.chain_kind,
        init_transfer_with_timestamp.tx_logs.clone(),
        &config,
    )
    .await;

    if vaa.is_none() {
        if init_transfer_with_timestamp.chain_kind == ChainKind::Eth {
            let Ok(light_client_latest_block_number) =
                utils::near::get_eth_light_client_last_block_number(&config, &jsonrpc_client).await
            else {
                warn!("Failed to get eth light client last block number");
                return;
            };

            if init_transfer_with_timestamp.block_number > light_client_latest_block_number {
                warn!("ETH light client is not synced yet");
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                ))
                .await;
                return;
            }
        } else {
            warn!("VAA is not ready yet");
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            return;
        }
    }

    let tx_hash = H256::from_slice(tx_hash.as_slice());

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &connector,
        init_transfer_with_timestamp.chain_kind,
        &recipient,
        &init_log.inner.tokenAddress.to_string(),
        init_log.inner.fee,
        init_log.inner.nativeFee,
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            warn!("{}", err);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::EVM_INIT_TRANSFER_EVENTS,
                &key,
            )
            .await;
            utils::redis::add_event(
                &mut redis_connection,
                utils::redis::STUCK_TRANSFERS,
                key,
                init_transfer_with_timestamp,
            )
            .await;
            return;
        }
    };

    let fin_transfer_args = if let Some(vaa) = vaa {
        omni_connector::FinTransferArgs::NearFinTransferWithVaa {
            chain_kind: init_transfer_with_timestamp.chain_kind,
            storage_deposit_actions,
            vaa,
        }
    } else {
        omni_connector::FinTransferArgs::NearFinTransferWithEvmProof {
            chain_kind: init_transfer_with_timestamp.chain_kind,
            tx_hash,
            storage_deposit_actions,
        }
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
        Err(err) => warn!("Failed to finalize InitTransfer: {}", err),
    }

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
    }
}
