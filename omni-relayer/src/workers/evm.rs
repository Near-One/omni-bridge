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
    ChainKind, OmniAddress,
};

use crate::{config, utils};

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    jsonrpc_client: near_jsonrpc_client::JsonRpcClient,
    near_signer: near_crypto::InMemorySigner,
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
                let vaa = utils::evm::get_vaa(tx_logs, &log, &config).await;

                if vaa.is_none() {
                    let Ok(light_client_latest_block_number) =
                        utils::near::get_eth_light_client_last_block_number(
                            &config,
                            &jsonrpc_client,
                        )
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
                    let near_signer = near_signer.clone();

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
                        let Ok(recipient) = init_log.inner.recipient.parse::<OmniAddress>() else {
                            warn!(
                                "Failed to parse recipient as OmniAddress: {:?}",
                                init_log.inner.recipient
                            );
                            return;
                        };

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

                        let storage_deposit_accounts =
                            if let OmniAddress::Near(near_recipient) = &recipient {
                                let Ok(recipient_account_id) = near_recipient.parse::<AccountId>()
                                else {
                                    warn!(
                                        "Failed to parse recipient as AccountId: {:?}",
                                        near_recipient
                                    );
                                    return;
                                };

                                let Ok(sender_has_storage_deposit) =
                                    utils::storage::has_storage_deposit(
                                        &jsonrpc_client,
                                        &token,
                                        &near_signer.account_id,
                                    )
                                    .await
                                else {
                                    warn!("Failed to check sender storage balance");
                                    return;
                                };
                                let Ok(recipient_has_storage_deposit) =
                                    utils::storage::has_storage_deposit(
                                        &jsonrpc_client,
                                        &token,
                                        &recipient_account_id,
                                    )
                                    .await
                                else {
                                    warn!("Failed to check recipient storage balance");
                                    return;
                                };

                                vec![
                                    (near_signer.account_id, !sender_has_storage_deposit),
                                    (recipient_account_id, !recipient_has_storage_deposit),
                                ]
                            } else {
                                Vec::new()
                            };

                        let fin_transfer_args = FinTransferArgs {
                            chain_kind: ChainKind::Eth,
                            native_fee_recipient: Some(config.evm.relayer_address_on_eth.clone()),
                            storage_deposit_args: StorageDepositArgs {
                                token,
                                accounts: storage_deposit_accounts,
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
