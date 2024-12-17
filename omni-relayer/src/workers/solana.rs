use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use log::{error, info, warn};

use omni_connector::OmniConnector;
use omni_types::{
    locker_args::StorageDepositAction, prover_args::WormholeVerifyProofArgs,
    prover_result::ProofKind, ChainKind, OmniAddress,
};

use crate::{config, utils};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct InitTransferWithTimestamp {
    pub amount: u128,
    pub token: String,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
    pub emitter: String,
    pub sequence: u64,
    pub creation_timestamp: i64,
    pub last_update_timestamp: Option<i64>,
}

pub async fn finalize_transfer(
    config: config::Config,
    redis_client: redis::Client,
    connector: Arc<OmniConnector>,
) -> Result<()> {
    let redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    loop {
        let mut redis_connection = redis_connection.clone();

        let events = match utils::redis::get_events(
            &mut redis_connection,
            utils::redis::SOLANA_INIT_TRANSFER_EVENTS.to_string(),
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

    if current_timestamp - init_transfer_with_timestamp.creation_timestamp
        > utils::redis::KEEP_INSUFFICIENT_FEE_TRANSFERS_FOR
    {
        warn!(
            "Removing an old InitTransfer on Solana: {:?}",
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

    info!("Received InitTransfer log on Solana");

    let recipient = match init_transfer_with_timestamp
        .recipient
        .parse::<OmniAddress>()
    {
        Ok(recipient) => recipient,
        Err(_) => {
            warn!(
                "Failed to parse recipient as OmniAddress: {:?}",
                init_transfer_with_timestamp.recipient
            );
            return;
        }
    };

    // TODO: Use existing API to check if fee is sufficient here

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

    let wormhole_proof_args = WormholeVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        vaa,
    };
    let Ok(prover_args) = borsh::to_vec(&wormhole_proof_args) else {
        warn!("Failed to serialize WormholeVerifyProofArgs");
        return;
    };

    let storage_deposit_actions =
        match get_storage_deposit_actions(&connector, &init_transfer_with_timestamp, &recipient)
            .await
        {
            Ok(actions) => actions,
            Err(err) => {
                warn!("{}", err);
                return;
            }
        };

    let fin_transfer_args = match recipient.get_chain() {
        ChainKind::Near => omni_connector::FinTransferArgs::NearFinTransfer {
            chain_kind: ChainKind::Sol,
            storage_deposit_actions,
            prover_args,
        },
        _ => todo!(),
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
    init_transfer_with_timestamp: &InitTransferWithTimestamp,
    recipient: &OmniAddress,
) -> Result<Vec<StorageDepositAction>, String> {
    let mut storage_deposit_actions = Vec::new();

    if let OmniAddress::Near(near_recipient) = recipient {
        let omni_token_address = OmniAddress::new_from_slice(
            ChainKind::Sol,
            init_transfer_with_timestamp.token.as_bytes(),
        )
        .map_err(|_| {
            format!(
                "Failed to convert EVM token address to OmniAddress: {:?}",
                init_transfer_with_timestamp.token
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

        let near_recipient_storage_deposit_amount = match connector
            .near_get_required_storage_deposit(token_id.clone(), near_recipient.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get required storage deposit for recipient: {:?}",
                    near_recipient
                )
            })? {
            amount if amount > 0 => Some(amount),
            _ => None,
        };

        storage_deposit_actions.push(StorageDepositAction {
            token_id,
            account_id: near_recipient.clone(),
            storage_deposit_amount: near_recipient_storage_deposit_amount,
        });
    };

    if init_transfer_with_timestamp.fee > 0 {
        let omni_token_address = OmniAddress::new_from_slice(
            ChainKind::Sol,
            init_transfer_with_timestamp.token.as_bytes(),
        )
        .map_err(|_| {
            format!(
                "Failed to convert EVM token address to OmniAddress: {:?}",
                init_transfer_with_timestamp.token
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

        let near_relayer_storage_deposit_amount = match connector
            .near_get_required_storage_deposit(token_id.clone(), relayer.clone())
            .await
            .map_err(|_| {
                format!(
                    "Failed to get required storage deposit for recipient: {:?}",
                    relayer
                )
            })? {
            amount if amount > 0 => Some(amount),
            _ => None,
        };

        storage_deposit_actions.push(StorageDepositAction {
            token_id,
            account_id: relayer,
            storage_deposit_amount: near_relayer_storage_deposit_amount,
        });
    }

    Ok(storage_deposit_actions)
}
