use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
};
use anyhow::Result;

const ETH_RPC_MAINNET: &str = "https://eth.llamarpc.com";
const BRIDGE_TOKEN_FACTORY_ADDRESS_MAINNET: &str = "0x252e87862A3A720287E7fd527cE6e8d0738427A2";

#[tokio::main]
async fn main() -> Result<()> {
    let provider = ProviderBuilder::new().on_http(ETH_RPC_MAINNET.parse().unwrap());

    let filter = Filter::new()
        .address(
            BRIDGE_TOKEN_FACTORY_ADDRESS_MAINNET
                .parse::<Address>()
                .unwrap(),
        )
        .event("Withdraw(string,address,uint256,string,address)")
        .from_block(20085270)
        .to_block(20085370);

    // Watch logs doesn't work because server returns "method eth_newFilter not supported"
    // TODO: Switch to infura
    let logs = provider.get_logs(&filter).await?;
    println!("poller: {:#?}", logs);

    Ok(())
}
