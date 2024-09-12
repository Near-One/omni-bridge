use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::{Filter, Log},
};

use crate::defaults;

pub async fn start_indexer(
    config: crate::Config,
    finalize_withdraw_tx: mpsc::UnboundedSender<Log>,
) -> Result<()> {
    let http_provider = ProviderBuilder::new().on_http(
        defaults::ETH_RPC_MAINNET
            .parse()
            .context("Failed to parse ETH rpc provider as url")?,
    );

    let ws_provider = ProviderBuilder::new()
        .on_ws(WsConnect::new(defaults::ETH_WS_MAINNET))
        .await
        .context("Failed to initialize WS provider")?;

    let latest_block = http_provider.get_block_number().await?;
    let from_block = latest_block.saturating_sub(1000);

    let filter = Filter::new()
        .address(config.bridge_token_factory_address_mainnet)
        .event("Withdraw(string,address,uint256,string,address)");

    let logs = http_provider
        .get_logs(&filter.clone().from_block(from_block).to_block(latest_block))
        .await?;
    for log in logs {
        if let Err(err) = finalize_withdraw_tx.send(log) {
            log::warn!("Failed to send log: {}", err);
        }
    }

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        if let Err(err) = finalize_withdraw_tx.send(log) {
            log::warn!("Failed to send log: {}", err);
        }
    }

    Ok(())
}
