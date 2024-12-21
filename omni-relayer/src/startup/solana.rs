use std::str::FromStr;

use anyhow::Result;
use log::{info, warn};
use tokio_stream::StreamExt;

use omni_types::ChainKind;
use solana_client::nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient};
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::signature::Signature;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::{config, utils};

pub async fn start_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let Some(solana_config) = config.solana else {
        anyhow::bail!("Failed to get Solana config");
    };

    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let rpc_http_url = &solana_config.rpc_http_url;
    let rpc_ws_url = &solana_config.rpc_ws_url;
    let program_id = Pubkey::from_str(&solana_config.program_id)?;

    let http_client = RpcClient::new(rpc_http_url.to_string());

    if let Err(e) =
        process_recent_signatures(&mut redis_connection, &http_client, &program_id).await
    {
        warn!("Failed to fetch recent logs: {}", e);
    }

    info!("All historical logs processed, starting Solana WS subscription");

    let Ok(ws_client) = PubsubClient::new(rpc_ws_url).await else {
        anyhow::bail!("Failed to connect to Solana WebSocket");
    };

    let filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);
    let config = RpcTransactionLogsConfig {
        commitment: Some(CommitmentConfig::processed()),
    };

    let Ok((mut log_stream, _)) = ws_client.logs_subscribe(filter, config).await else {
        anyhow::bail!("Failed to subscribe to Solana logs");
    };

    info!("Subscribed to live Solana logs");

    while let Some(log) = log_stream.next().await {
        let Ok(signature) = Signature::from_str(&log.value.signature) else {
            warn!("Failed to parse signature: {:?}", log.value.signature);
            continue;
        };

        info!("Found a signature on Solana: {:?}", signature);

        utils::redis::add_event(
            &mut redis_connection,
            utils::redis::SOLANA_EVENTS,
            signature.to_string(),
            // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
            serde_json::Value::Null,
        )
        .await;
    }

    Ok(())
}

async fn process_recent_signatures(
    redis_connection: &mut redis::aio::MultiplexedConnection,
    http_client: &RpcClient,
    program_id: &Pubkey,
) -> Result<()> {
    let Some(mut last_signature) = utils::redis::get_last_processed::<&str, String>(
        redis_connection,
        &utils::redis::get_last_processed_key(ChainKind::Sol).await,
    )
    .await
    .map(|s| Signature::from_str(&s).ok()) else {
        return Ok(());
    };

    loop {
        let signatures: Vec<RpcConfirmedTransactionStatusWithSignature> = http_client
            .get_signatures_for_address_with_config(
                program_id,
                GetConfirmedSignaturesForAddress2Config {
                    limit: None,
                    before: None,
                    until: last_signature,
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        if signatures.is_empty() {
            break;
        }

        for signature_status in &signatures {
            utils::redis::add_event(
                redis_connection,
                utils::redis::SOLANA_EVENTS,
                signature_status.signature.clone(),
                // TODO: It's better to come up with a solution that wouldn't require storing `Null` value
                serde_json::Value::Null,
            )
            .await;
        }

        last_signature = signatures
            .last()
            .and_then(|signature| Signature::from_str(&signature.signature).ok());
    }

    Ok(())
}
