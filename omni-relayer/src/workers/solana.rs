use std::{str::FromStr, sync::Arc};

use anyhow::Result;
use futures::future::join_all;
use log::{info, warn};

use near_bridge_client::TransactionOptions;
use near_primitives::views::TxExecutionStatus;
use omni_connector::OmniConnector;
#[cfg(not(feature = "disable_fee_check"))]
use omni_types::Fee;
use omni_types::{ChainKind, OmniAddress};
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcTransactionConfig};
#[cfg(not(feature = "disable_fee_check"))]
use solana_sdk::pubkey::Pubkey;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};
use solana_transaction_status::{UiMessage, UiTransactionEncoding};

use crate::{config, utils};

pub async fn process_signature(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let Some(solana_config) = config.solana else {
        anyhow::bail!("Failed to get Solana config");
    };

    let rpc_http_url = &solana_config.rpc_http_url;
    let http_client = Arc::new(RpcClient::new(rpc_http_url.to_string()));

    let fetching_config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();

        let Some(events) = utils::redis::get_events(
            &mut redis_connection,
            utils::redis::SOLANA_EVENTS.to_string(),
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

        for (key, _) in events {
            handlers.push(tokio::spawn({
                let mut redis_connection = redis_connection.clone();
                let solana = solana_config.clone();
                let http_client = http_client.clone();

                async move {
                    let Ok(signature) = Signature::from_str(&key) else {
                        warn!("Failed to parse signature: {:?}", key);
                        return;
                    };

                    info!("Trying to process signature: {:?}", signature);

                    match http_client
                        .get_transaction_with_config(&signature, fetching_config)
                        .await
                    {
                        Ok(tx) => {
                            let transaction = tx.transaction;

                            if let solana_transaction_status::EncodedTransaction::Json(ref tx) =
                                transaction.transaction
                            {
                                if let UiMessage::Raw(ref raw) = tx.message {
                                    utils::solana::process_message(
                                        &mut redis_connection,
                                        &solana,
                                        &transaction,
                                        raw,
                                        signature,
                                    )
                                    .await;
                                }
                            }

                            utils::redis::remove_event(
                                &mut redis_connection,
                                utils::redis::SOLANA_EVENTS,
                                &signature.to_string(),
                            )
                            .await;
                            utils::redis::update_last_processed(
                                &mut redis_connection,
                                &utils::redis::get_last_processed_key(ChainKind::Sol),
                                &signature.to_string(),
                            )
                            .await;
                        }
                        Err(err) => {
                            warn!("Failed to fetch transaction (probably signature wasn't finalized yet): {}", err);
                        }
                    };
                }
            }));
        }

        join_all(handlers).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(
            utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
        ))
        .await;
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub amount: u128,
    pub token: String,
    pub sender: String,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
    pub message: String,
    pub emitter: String,
    pub sequence: u64,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
    near_nonce: Arc<utils::nonce::NonceManager>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();

        let Some(events) = utils::redis::get_events(
            &mut redis_connection,
            utils::redis::SOLANA_INIT_TRANSFER_EVENTS.to_string(),
        )
        .await
        else {
            tokio::time::sleep(tokio::time::Duration::from_secs(
                utils::redis::SLEEP_TIME_AFTER_EVENTS_PROCESS_SECS,
            ))
            .await;
            continue;
        };

        if let Err(err) = near_nonce.resync_nonce().await {
            warn!("Failed to resync nonce: {}", err);
        }

        let mut handlers = Vec::new();

        for (key, event) in events {
            if let Ok(init_transfer_with_timestamp) =
                serde_json::from_str::<InitTransferWithTimestamp>(&event)
            {
                let handler = handle_init_transfer_event(
                    config.clone(),
                    connector.clone(),
                    near_nonce.clone(),
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
    near_nonce: Arc<utils::nonce::NonceManager>,
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

    info!("Trying to process InitTransfer log on Solana");

    let Ok(recipient) = init_transfer_with_timestamp
        .recipient
        .parse::<OmniAddress>()
    else {
        warn!(
            "Failed to parse recipient as OmniAddress: {:?}",
            init_transfer_with_timestamp.recipient
        );
        return;
    };

    #[cfg(not(feature = "disable_fee_check"))]
    {
        let Ok(sender) = Pubkey::from_str(&init_transfer_with_timestamp.sender) else {
            warn!(
                "Failed to parse sender address as Pubkey: {:?}",
                init_transfer_with_timestamp.sender
            );
            return;
        };
        let Ok(sender) = OmniAddress::new_from_slice(ChainKind::Sol, &sender.to_bytes()) else {
            warn!(
                "Failed to convert sender address to OmniAddress: {:?}",
                init_transfer_with_timestamp.sender
            );
            return;
        };

        let Ok(token) = Pubkey::from_str(&init_transfer_with_timestamp.token) else {
            warn!(
                "Failed to parse token address as Pubkey: {:?}",
                init_transfer_with_timestamp.token
            );
            return;
        };
        let Ok(token) = OmniAddress::new_from_slice(ChainKind::Sol, &token.to_bytes()) else {
            warn!(
                "Failed to convert token address to OmniAddress: {:?}",
                init_transfer_with_timestamp.token
            );
            return;
        };

        match utils::fee::is_fee_sufficient(
            &config,
            Fee {
                fee: init_transfer_with_timestamp.fee.into(),
                native_fee: u128::from(init_transfer_with_timestamp.native_fee).into(),
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

    let Ok(vaa) = connector
        .wormhole_get_vaa(
            config.wormhole.solana_chain_id,
            &init_transfer_with_timestamp.emitter,
            init_transfer_with_timestamp.sequence,
        )
        .await
    else {
        warn!(
            "Failed to get VAA for sequence: {}",
            init_transfer_with_timestamp.sequence
        );
        return;
    };

    let storage_deposit_actions = match utils::storage::get_storage_deposit_actions(
        &connector,
        ChainKind::Sol,
        &recipient,
        &init_transfer_with_timestamp.token,
        init_transfer_with_timestamp.fee,
        u128::from(init_transfer_with_timestamp.native_fee),
    )
    .await
    {
        Ok(actions) => actions,
        Err(err) => {
            warn!("{}", err);
            utils::redis::add_event(
                &mut redis_connection,
                utils::redis::STUCK_TRANSFERS,
                &key,
                init_transfer_with_timestamp,
            )
            .await;
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::SOLANA_INIT_TRANSFER_EVENTS,
                &key,
            )
            .await;
            return;
        }
    };

    let nonce = match near_nonce.reserve_nonce().await {
        Ok(nonce) => Some(nonce),
        Err(err) => {
            warn!("Failed to reserve nonce: {}", err);
            return;
        }
    };

    let fin_transfer_args = omni_connector::FinTransferArgs::NearFinTransferWithVaa {
        chain_kind: ChainKind::Sol,
        storage_deposit_actions,
        vaa,
        transaction_options: TransactionOptions {
            nonce,
            wait_until: TxExecutionStatus::Included,
        },
    };

    match connector.fin_transfer(fin_transfer_args).await {
        Ok(tx_hash) => {
            info!("Finalized InitTransfer: {:?}", tx_hash);
            utils::redis::remove_event(
                &mut redis_connection,
                utils::redis::SOLANA_INIT_TRANSFER_EVENTS,
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
            "Removing an old InitTransfer on Solana: {:?}",
            init_transfer_with_timestamp
        );
        utils::redis::remove_event(
            &mut redis_connection,
            utils::redis::SOLANA_INIT_TRANSFER_EVENTS,
            &key,
        )
        .await;
    }
}
