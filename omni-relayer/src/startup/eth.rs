use anyhow::{Context, Result};
use log::info;
use tokio_stream::StreamExt;

use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{Filter, Log},
    sol,
};

use crate::{config, utils};

sol!(
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event Withdraw(
        string token,
        address indexed sender,
        uint256 amount,
        string recipient,
        address indexed tokenEthAddress
    );

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    event Deposit(
        string token,
        uint256 amount,
        address recipient,
        uint128 indexed nonce,
        string feeRecipient
    );
);

pub async fn start_indexer(config: config::Config, redis_client: redis::Client) -> Result<()> {
    let mut redis_connection = redis_client.get_multiplexed_tokio_connection().await?;

    let http_provider = ProviderBuilder::new().on_http(
        config
            .eth
            .rpc_http_url
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(config.eth.rpc_ws_url.clone()))
        .await
        .context("Failed to initialize WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block =
        utils::redis::get_last_processed_block(&mut redis_connection, "eth_last_processed_block")
            .await
            .map_or_else(|| latest_block.saturating_sub(10_000), |block| block);

    let filter = Filter::new()
        .address(config.eth.bridge_token_factory_address)
        .events(vec![
            "Withdraw(string,address,uint256,string,address)",
            "Deposit(string,uint256,address,uint128,string)",
        ]);

    for current_block in (from_block..latest_block).step_by(10_000) {
        let logs = http_provider
            .get_logs(
                &filter
                    .clone()
                    .from_block(current_block)
                    .to_block(current_block + 10_000),
            )
            .await?;

        for log in logs {
            process_log(&mut redis_connection, &log).await;
        }
    }

    info!("All historical logs processed, starting WS subscription");

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        process_log(&mut redis_connection, &log).await;
    }

    Ok(())
}

async fn process_log(redis_connection: &mut redis::aio::MultiplexedConnection, log: &Log) {
    if let Some(block_height) = log.block_number {
        utils::redis::update_last_processed_block(
            redis_connection,
            utils::redis::ETH_LAST_PROCESSED_BLOCK,
            block_height,
        )
        .await;
    }

    if let Some(tx_hash) = log.transaction_hash {
        if let Ok(withdraw_log) = log.log_decode::<Withdraw>() {
            utils::redis::add_event(
                redis_connection,
                utils::redis::ETH_WITHDRAW_EVENTS,
                tx_hash.to_string(),
                withdraw_log,
            )
            .await;
        } else if let Ok(deposit_log) = log.log_decode::<Deposit>() {
            utils::redis::add_event(
                redis_connection,
                utils::redis::FINALISED_TRANSFERS,
                tx_hash.to_string(),
                deposit_log,
            )
            .await;
        }
    }
}
