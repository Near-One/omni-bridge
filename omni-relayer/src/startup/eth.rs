use std::sync::Arc;

use anyhow::{Context, Result};
use tokio_stream::StreamExt;

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Filter,
};

use nep141_connector::Nep141Connector;

use crate::{defaults, utils};

pub async fn start_indexer(connector: Arc<Nep141Connector>) -> Result<()> {
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
        .address(defaults::BRIDGE_TOKEN_FACTORY_ADDRESS_MAINNET.parse::<Address>()?)
        .event("Withdraw(string,address,uint256,string,address)");

    let logs = http_provider
        .get_logs(&filter.clone().from_block(from_block).to_block(latest_block))
        .await?;
    for log in logs {
        utils::eth::process_log(connector.clone(), log).await;
    }

    let mut stream = ws_provider.subscribe_logs(&filter).await?.into_stream();
    while let Some(log) = stream.next().await {
        utils::eth::process_log(connector.clone(), log).await;
    }

    Ok(())
}
