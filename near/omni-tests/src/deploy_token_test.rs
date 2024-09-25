use near_crypto::{InMemorySigner, KeyType};
use near_primitives::views::FinalExecutionStatus;
use dotenv::dotenv;
use std::env;
use std::path::Path;
use std::ffi::OsStr;
use std::str::FromStr;
use serde_json::json;
use tracing::info;

#[macro_use]
extern crate serde_json;

pub fn init_logger() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    let _result = tracing::subscriber::set_global_default(subscriber);
}

#[tokio::test]
async fn deploy_token_test() {
    init_logger();

    dotenv().unwrap();

    let near_signer = get_near_signer();
    let nep141_connector = build_connector(&near_signer);
    let mock_token = env::var("MOCK_TOKEN_ACCOUNT_ID").unwrap();
    let tx_id = nep141_connector.log_token_metadata(mock_token).await.unwrap();
    let eth_tx = nep141_connector.new_bridge_token_omni(tx_id, None).await.unwrap();

    info!("Eth tx: {:?}", eth_tx)
}

fn abspath(p: &str) -> Option<String> {
    shellexpand::full(p)
        .ok()
        .and_then(|x| Path::new(OsStr::new(x.as_ref())).canonicalize().ok())
        .and_then(|p| p.into_os_string().into_string().ok())
}

fn get_near_signer() -> InMemorySigner {
    let path = format!("~/.near-credentials/testnet/{}.json", env::var("SIGNER_ACCOUNT_ID").unwrap());
    let absolute = abspath(&path).unwrap();
    read_private_key_from_file(&absolute).unwrap()
}

fn get_near_endpoint_url() -> url::Url {
    url::Url::parse("https://rpc.testnet.near.org").unwrap()
}

fn read_private_key_from_file(
    absolute_path: &str,
) -> Result<near_crypto::InMemorySigner, String> {
    let data = std::fs::read_to_string(absolute_path)
        .map_err(|e| format!("Unable to read file {}: {}", absolute_path, e))?;
    let res: serde_json::Value = serde_json::from_str(&data)
        .map_err(|e| format!("Unable to parse {}: {}", absolute_path, e))?;

    let private_key = res["private_key"].to_string().replace('\"', "");
    let private_key =
        near_crypto::SecretKey::from_str(private_key.as_str()).map_err(|e| e.to_string())?;

    let account_id = res["account_id"].to_string().replace('\"', "");
    let account_id = near_primitives::types::AccountId::from_str(account_id.as_str())
        .map_err(|e| e.to_string())?;

    Ok(near_crypto::InMemorySigner::from_secret_key(
        account_id,
        private_key,
    ))
}

pub fn build_connector(
    near_signer: &InMemorySigner,
) -> nep141_connector::Nep141Connector {
    nep141_connector::Nep141ConnectorBuilder::default()
        .eth_endpoint(Some("https://eth.llamarpc.com".to_string()))
        .eth_chain_id(Some(11_155_111))
        .near_endpoint(Some(get_near_endpoint_url().to_string()))
        .token_locker_id(Some(env::var("NEP141_LOCKER_ACCOUNT_ID").unwrap()))
        .bridge_token_factory_address(Some(env::var("ETH_BRIDGE_TOKEN_FACTORY_ADDRESS").unwrap()))
        .near_light_client_address(Some("0x202cdf10bfa45a3d2190901373edd864f071d707".to_string()))
        .eth_private_key(Some(std::env::var("ETH_PRIVATE_KEY").unwrap()))
        .near_signer(Some(near_signer.account_id.to_string()))
        .near_private_key(Some(near_signer.secret_key.to_string()))
        .build().unwrap()
}
