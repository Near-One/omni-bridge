use std::sync::Arc;

use alloy::rpc::types::{Log, TransactionReceipt};
use anyhow::Result;
use ethereum_types::H256;
use futures::future::join_all;
use log::{error, info, warn};

use near_primitives::types::AccountId;
use omni_connector::OmniConnector;
use omni_types::{
    locker_args::{FinTransferArgs, StorageDepositArgs},
    near_events::Nep141LockerEvent,
    ChainKind,
};

use crate::{config, utils};

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::ETH_WITHDRAW_EVENTS.to_string(),
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
            if let Ok((block_number, log, tx_logs)) =
                serde_json::from_str::<(u64, Log, Option<TransactionReceipt>)>(&event)
            {
                let Ok(light_client_latest_block_number) =
                    utils::near::get_eth_light_client_last_block_number(&config, &jsonrpc_client)
                        .await
                else {
                    warn!("Failed to get eth light client last block number");
                    continue;
                };

                if block_number > light_client_latest_block_number {
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
                    ))
                    .await;
                    continue;
                }

                let Ok(init_log) = log.log_decode::<utils::evm::InitTransfer>() else {
                    warn!("Failed to decode log as InitTransfer: {:?}", log);
                    continue;
                };

                handlers.push(tokio::spawn({
                    let config = config.clone();
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();
                    let jsonrpc_client = jsonrpc_client.clone();

                    async move {
                        info!("Received InitTransfer log");

                        let Some(tx_hash) = log.transaction_hash else {
                            warn!("No transaction hash in log: {:?}", log);
                            return;
                        };

                        let Some(topic) = log.topic0() else {
                            warn!("No topic0 in log: {:?}", log);
                            return;
                        };

                        let tx_hash = H256::from_slice(tx_hash.as_slice());

                        let Ok(token) = init_log.inner.token.parse::<AccountId>() else {
                            warn!(
                                "Failed to parse token as AccountId: {:?}",
                                init_log.inner.token
                            );
                            return;
                        };
                        let Ok(recipient) = init_log.inner.recipient.parse::<AccountId>() else {
                            warn!(
                                "Failed to parse recipient as AccountId: {:?}",
                                init_log.inner.recipient
                            );
                            return;
                        };

                        let vaa = utils::evm::get_vaa(tx_logs, &log, &config).await;

                        let Some(prover_args) = utils::evm::get_prover_args(
                            vaa,
                            tx_hash,
                            H256::from_slice(topic.as_slice()),
                            &config,
                        )
                        .await
                        else {
                            return;
                        };

                        let sender = config.near.token_locker_id.clone();

                        // If storage is sufficient, then flag should be false, otherwise true
                        let sender_is_storage_deposit = !utils::storage::is_storage_sufficient(
                            &jsonrpc_client,
                            &token,
                            &sender,
                        )
                        .await
                        .unwrap_or_default();
                        let recipient_is_storage_deposit = !utils::storage::is_storage_sufficient(
                            &jsonrpc_client,
                            &token,
                            &recipient,
                        )
                        .await
                        .unwrap_or_default();

                        let fin_transfer_args = FinTransferArgs {
                            chain_kind: ChainKind::Eth,
                            native_fee_recipient: config.evm.relayer_address_on_eth.clone(),
                            storage_deposit_args: StorageDepositArgs {
                                token,
                                accounts: vec![
                                    (sender, sender_is_storage_deposit),
                                    (recipient, recipient_is_storage_deposit),
                                ],
                            },
                            prover_args,
                        };

                        match connector.near_fin_transfer(fin_transfer_args).await {
                            Ok(tx_hash) => {
                                info!("Finalized InitTransfer: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::ETH_WITHDRAW_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to finalize InitTransfer: {}", err),
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

pub async fn claim_native_fee(
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection_clone = redis_connection.clone();
        let Some(events) = utils::redis::get_events(
            &mut redis_connection_clone,
            utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_EVENTS.to_string(),
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
            if let Ok(event) = serde_json::from_str::<Nep141LockerEvent>(&event) {
                handlers.push(tokio::spawn({
                    let mut redis_connection = redis_connection.clone();
                    let connector = connector.clone();

                    async move {
                        let Nep141LockerEvent::SignClaimNativeFeeEvent { .. } = event else {
                            warn!("Expected SignClaimNativeFeeEvent, got: {:?}", event);
                            return;
                        };

                        info!("Received SignClaimNativeFeeEvent log");

                        match connector.evm_claim_native_fee_with_log(event).await {
                            Ok(tx_hash) => {
                                info!("Claimed native fee: {:?}", tx_hash);
                                utils::redis::remove_event(
                                    &mut redis_connection,
                                    utils::redis::NEAR_SIGN_CLAIM_NATIVE_FEE_EVENTS,
                                    key,
                                )
                                .await;
                            }
                            Err(err) => error!("Failed to claim native fee: {}", err),
                        }
                    }
                }));
            }
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}
