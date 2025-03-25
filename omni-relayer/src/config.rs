use alloy::{
    primitives::Address,
    signers::{k256::ecdsa::SigningKey, local::LocalSigner},
};
use anyhow::Context;
use near_primitives::types::AccountId;
use near_sdk::base64::{prelude::BASE64_STANDARD, Engine};
use omni_types::{ChainKind, OmniAddress};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithHttpConfig as _;
use opentelemetry_sdk::{
    metrics::{Aggregation, Instrument, PeriodicReader, SdkMeterProvider, Stream, Temporality},
    Resource,
};
use serde::Deserialize;

pub fn get_private_key(chain_kind: ChainKind) -> String {
    let env_var = match chain_kind {
        ChainKind::Near => "NEAR_PRIVATE_KEY",
        ChainKind::Eth => "ETH_PRIVATE_KEY",
        ChainKind::Base => "BASE_PRIVATE_KEY",
        ChainKind::Arb => "ARB_PRIVATE_KEY",
        ChainKind::Sol => "SOLANA_PRIVATE_KEY",
    };

    std::env::var(env_var).unwrap_or_else(|_| panic!("Failed to get `{env_var}` env variable"))
}

pub fn get_relayer_evm_address(chain_kind: ChainKind) -> Address {
    let decoded_private_key =
        hex::decode(get_private_key(chain_kind)).expect("Failed to decode EVM private key");

    let secret_key = SigningKey::from_slice(&decoded_private_key)
        .expect("Failed to create a `SecretKey` from the provided private key");

    let signer = LocalSigner::from_signing_key(secret_key);

    signer.address()
}

fn replace_mongodb_credentials<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let uri = Option::<String>::deserialize(deserializer)?;

    if let Some(uri) = uri {
        let username = std::env::var("MONGODB_USERNAME").map_err(serde::de::Error::custom)?;
        let password = std::env::var("MONGODB_PASSWORD").map_err(serde::de::Error::custom)?;
        let host = std::env::var("MONGODB_HOST").map_err(serde::de::Error::custom)?;

        Ok(Some(
            uri.replace("MONGODB_USERNAME", &username)
                .replace("MONGODB_PASSWORD", &password)
                .replace("MONGODB_HOST", &host),
        ))
    } else {
        Ok(None)
    }
}

fn replace_rpc_api_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let url = String::deserialize(deserializer)?;

    let api_key = std::env::var("INFURA_API_KEY").map_err(serde::de::Error::custom)?;

    Ok(url.replace("INFURA_API_KEY", &api_key))
}

fn validate_fee_discount<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let fee_discount = u8::deserialize(deserializer)?;

    if fee_discount > 100 {
        return Err(serde::de::Error::custom(
            "Fee discount should be less than 100",
        ));
    }

    Ok(fee_discount)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub redis: Redis,
    pub bridge_indexer: BridgeIndexer,
    pub near: Near,
    pub eth: Option<Evm>,
    pub base: Option<Evm>,
    pub arb: Option<Evm>,
    pub solana: Option<Solana>,
    pub wormhole: Wormhole,
}

impl Config {
    pub fn is_bridge_indexer_enabled(&self) -> bool {
        self.bridge_indexer.mongodb_uri.is_some() && self.bridge_indexer.db_name.is_some()
    }

    pub fn is_bridge_api_enabled(&self) -> bool {
        self.bridge_indexer.api_url.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Redis {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeIndexer {
    pub api_url: Option<String>,

    #[serde(default, deserialize_with = "replace_mongodb_credentials")]
    pub mongodb_uri: Option<String>,
    pub db_name: Option<String>,

    #[serde(default, deserialize_with = "validate_fee_discount")]
    pub fee_discount: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Testnet,
    Mainnet,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Near {
    pub network: Network,
    pub rpc_url: String,
    pub token_locker_id: AccountId,
    pub credentials_path: Option<String>,
    pub sign_without_checking_fee: Option<Vec<OmniAddress>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Evm {
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_http_url: String,
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_ws_url: String,
    pub chain_id: u64,
    pub bridge_token_factory_address: Address,
    pub light_client: Option<AccountId>,
    pub block_processing_batch_size: u64,
    pub expected_finalization_time: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Solana {
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_http_url: String,
    #[serde(deserialize_with = "replace_rpc_api_key")]
    pub rpc_ws_url: String,
    pub program_id: String,
    pub wormhole_id: String,
    pub deploy_token_emitter_index: usize,
    pub deploy_token_discriminator: Vec<u8>,
    pub init_transfer_sender_index: usize,
    pub init_transfer_token_index: usize,
    pub init_transfer_emitter_index: usize,
    pub init_transfer_sol_sender_index: usize,
    pub init_transfer_sol_emitter_index: usize,
    pub init_transfer_discriminator: Vec<u8>,
    pub init_transfer_sol_discriminator: Vec<u8>,
    pub finalize_transfer_emitter_index: usize,
    pub finalize_transfer_sol_emitter_index: usize,
    pub finalize_transfer_discriminator: Vec<u8>,
    pub finalize_transfer_sol_discriminator: Vec<u8>,
    pub credentials_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Wormhole {
    pub api_url: String,
    pub solana_chain_id: u64,
}

async fn get_gcp_instance_id() -> anyhow::Result<String> {
    Ok(reqwest::Client::new()
        .get("http://metadata.google.internal/computeMetadata/v1/instance/id")
        .header("Metadata-Flavor", "Google")
        .send()
        .await?
        .text()
        .await?)
}

pub async fn get_meter_provider() -> anyhow::Result<SdkMeterProvider> {
    let metrics_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_headers(std::collections::HashMap::from([(
            "Authorization".to_string(),
            // https://datatracker.ietf.org/doc/html/rfc7617#section-2
            format!(
                "Basic {}",
                BASE64_STANDARD.encode(format!(
                    "{}:{}",
                    std::env::var("GRAFANA_CLOUD_INSTANCE_ID")
                        .context("GRAFANA_CLOUD_INSTANCE_ID env variable is not set")?,
                    std::env::var("GRAFANA_CLOUD_API_KEY")
                        .context("GRAFANA_CLOUD_API_KEY env variable is not set")?,
                ))
            ),
        )]))
        .with_temporality(Temporality::default())
        .build()?;

    let reader = PeriodicReader::builder(
        metrics_exporter,
        opentelemetry_sdk::runtime::TokioCurrentThread,
    )
    .build();

    Ok(SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(Resource::new([
            // K_SERVICE and K_REVISION should be set by cloud run, so we provide defaults for
            // local runs.
            // https://cloud.google.com/run/docs/container-contract#services-env-vars
            KeyValue::new("service.name", std::env::var("K_SERVICE").context("K_SERVICE env variable is not set")?),
            KeyValue::new("service.version", std::env::var("K_REVISION").context("K_REVISION env variable is not set")?),
            KeyValue::new("service.namespace", std::env::var("SERVICE_NAMESPACE").context("SERVICE_NAMESPACE env variable is not set")?),
            KeyValue::new(
                "service.instance.id",
                get_gcp_instance_id().await.unwrap_or_else(|err| {
                    log::warn!("Failed to get instance id. Shouldn't happen if running in GCP. Error: {err:?}");
                    "local-instance".to_string()
                }),
            ),
        ]))
        .with_view(opentelemetry_sdk::metrics::new_view(
            // https://github.com/open-telemetry/semantic-conventions/blob/b865f63bc7ba3039ad87504d75ee40104149d1bd/docs/http/http-metrics.md#metric-httpserverrequestduration
            Instrument::new().name("http.server.duration"),
            Stream::new().aggregation(Aggregation::ExplicitBucketHistogram {
                boundaries: vec![
                    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
                ],
                record_min_max: true,
            }),
        )?)
        .build())
}
