use alloy::{
    consensus::Transaction as TransactionTrait,
    eips::eip1559::Eip1559Estimation,
    network::EthereumWallet,
    providers::{Provider, ProviderBuilder, WalletProvider},
    rpc::types::Transaction,
};
use anyhow::Context;
use omni_types::ChainKind;
use tokio::time::{Duration, Sleep};
use tracing::{info, warn};

use crate::{
    config::{self, Evm},
    utils::{
        self,
        pending_transactions::{self, PendingTransaction},
    },
    workers::RetryableEvent,
};

enum ShouldBump {
    Yes(Eip1559Estimation),
    No(String),
}

#[allow(dead_code)]
enum TransactionStatus {
    Included(Transaction),
    Pending(Transaction),
    Missing,
}

pub async fn start_evm_fee_bumping(
    config: config::Config,
    chain_kind: ChainKind,
    redis_connection_manager: &mut redis::aio::ConnectionManager,
) -> anyhow::Result<()> {
    let evm_config = match chain_kind {
        ChainKind::Eth => &config.eth,
        ChainKind::Bnb => &config.bnb,
        ChainKind::Arb => &config.arb,
        ChainKind::Base => &config.base,
        _ => {
            return Err(anyhow::anyhow!(
                "EVM fee bumping not supported for chain kind: {chain_kind:?}"
            ));
        }
    };
    let evm_config = evm_config
        .as_ref()
        .context(format!("{chain_kind:?} config not found in global config"))?;
    let fee_bumping_config = evm_config
        .fee_bumping
        .as_ref()
        .context(format!("Fee bumping config not found for {chain_kind:?}"))?;

    let provider = build_provider(chain_kind, evm_config).context("Error building EVM provider")?;

    info!(
        "Fee bumping for {chain_kind:?} initialized with address: {}",
        provider.default_signer_address()
    );

    loop {
        let redis_key = pending_transactions::get_pending_tx_key(chain_kind);

        let Some(pending_txs) = utils::redis::zrange::<PendingTransaction>(
            &config,
            redis_connection_manager,
            &redis_key,
            0,
            0,
        )
        .await
        else {
            sleep(fee_bumping_config.check_interval_seconds).await;
            continue;
        };

        if pending_txs.is_empty() {
            sleep(fee_bumping_config.check_interval_seconds).await;
            continue;
        }

        let mut pending_tx = pending_txs[0].clone();

        let current_timestamp = chrono::Utc::now().timestamp();
        if current_timestamp - pending_tx.sent_timestamp
            < fee_bumping_config.min_pending_time_seconds
            || pending_tx.last_bump_timestamp.is_some_and(|last_bump| {
                current_timestamp - last_bump < fee_bumping_config.min_since_last_bump_seconds
            })
        {
            sleep(fee_bumping_config.check_interval_seconds).await;
            continue;
        }

        let tx_status = match get_transaction_status(&provider, &pending_tx.tx_hash).await {
            Ok(status) => status,
            Err(err) => {
                warn!(
                    "Error checking transaction {} status for {chain_kind:?}: {err:?}",
                    pending_tx.tx_hash
                );
                sleep(fee_bumping_config.check_interval_seconds).await;
                continue;
            }
        };

        match tx_status {
            TransactionStatus::Included(_) => {
                utils::redis::zrem(&config, redis_connection_manager, &redis_key, pending_tx).await;
            }
            TransactionStatus::Missing => {
                info!(
                    "Resending source event of missing transaction {} (nonce: {}) on {chain_kind:?}",
                    pending_tx.tx_hash, pending_tx.nonce
                );

                utils::redis::add_event(
                    &config,
                    redis_connection_manager,
                    utils::redis::EVENTS,
                    &pending_tx.source_event_id,
                    RetryableEvent::new(&pending_tx.source_event),
                )
                .await;

                utils::redis::zrem(&config, redis_connection_manager, &redis_key, pending_tx).await;
            }
            TransactionStatus::Pending(tx_data) => {
                let original_fee = Eip1559Estimation {
                    max_fee_per_gas: tx_data.max_fee_per_gas(),
                    max_priority_fee_per_gas: tx_data.max_priority_fee_per_gas().unwrap_or(0),
                };
                let suggested_fee = provider.estimate_eip1559_fees().await?;

                let new_fee = match should_bump(fee_bumping_config, original_fee, suggested_fee) {
                    ShouldBump::Yes(new_fee) => new_fee,
                    ShouldBump::No(reason) => {
                        info!(
                            "Not bumping fee for transaction {} on {chain_kind:?}: {}",
                            pending_tx.tx_hash, reason
                        );
                        sleep(fee_bumping_config.check_interval_seconds).await;
                        continue;
                    }
                };

                info!(
                    "Bumping fee for transaction {} (nonce: {}) on {chain_kind:?}: {:.3} -> {:.3} gwei",
                    pending_tx.tx_hash,
                    pending_tx.nonce,
                    wei_to_gwei(original_fee.max_fee_per_gas),
                    wei_to_gwei(new_fee.max_fee_per_gas)
                );

                let replacement_tx = tx_data
                    .into_request()
                    .max_fee_per_gas(new_fee.max_fee_per_gas)
                    .max_priority_fee_per_gas(new_fee.max_priority_fee_per_gas);

                let new_tx_result = match provider.send_transaction(replacement_tx).await {
                    Ok(result) => result,
                    Err(err) => {
                        warn!(
                            "Error sending replacement transaction for {} on {chain_kind:?}: {err:?}",
                            pending_tx.tx_hash
                        );
                        sleep(fee_bumping_config.check_interval_seconds).await;
                        continue;
                    }
                };

                let new_tx_hash = new_tx_result.tx_hash();

                info!(
                    "Replacement transaction sent: {} (replaced {}) for {chain_kind:?}",
                    new_tx_hash, pending_tx.tx_hash
                );

                utils::redis::zrem(
                    &config,
                    redis_connection_manager,
                    &redis_key,
                    pending_tx.clone(),
                )
                .await;

                pending_tx.bump(new_tx_hash.to_string());

                utils::redis::zadd(
                    &config,
                    redis_connection_manager,
                    &redis_key,
                    pending_tx.nonce,
                    pending_tx,
                )
                .await;

                sleep(fee_bumping_config.check_interval_seconds).await;
            }
        }
    }
}

fn should_bump(
    fee_bumping_config: &config::FeeBumping,
    original_fee: Eip1559Estimation,
    suggested_fee: Eip1559Estimation,
) -> ShouldBump {
    if suggested_fee.max_fee_per_gas < original_fee.max_fee_per_gas {
        return ShouldBump::No(format!(
            "new fee ({:.3} gwei) is lower than original ({:.3} gwei)",
            wei_to_gwei(suggested_fee.max_fee_per_gas),
            wei_to_gwei(original_fee.max_fee_per_gas),
        ));
    }

    let min_bump_multiplier: u128 = (100 + fee_bumping_config.min_fee_increase_percent).into();
    let min_max_fee = (original_fee.max_fee_per_gas * min_bump_multiplier) / 100;
    let min_priority_fee = (original_fee.max_priority_fee_per_gas * min_bump_multiplier) / 100;

    // Nodes generally required minimum 10-15% fee bump
    let new_max_fee = min_max_fee.max(suggested_fee.max_fee_per_gas);
    let new_priority_fee = min_priority_fee.max(suggested_fee.max_priority_fee_per_gas);

    if new_max_fee > fee_bumping_config.max_fee_in_wei {
        return ShouldBump::No(format!(
            "bumped fee ({:.3} gwei) exceeds configured maximum ({:.3} gwei)",
            wei_to_gwei(new_max_fee),
            wei_to_gwei(fee_bumping_config.max_fee_in_wei),
        ));
    }

    ShouldBump::Yes(Eip1559Estimation {
        max_fee_per_gas: new_max_fee,
        max_priority_fee_per_gas: new_priority_fee,
    })
}

async fn get_transaction_status<P>(provider: &P, tx_hash: &str) -> anyhow::Result<TransactionStatus>
where
    P: Provider,
{
    match provider.get_transaction_by_hash(tx_hash.parse()?).await {
        Ok(Some(tx)) => {
            if tx.block_number.is_some() {
                Ok(TransactionStatus::Included(tx))
            } else {
                Ok(TransactionStatus::Pending(tx))
            }
        }
        Ok(None) => Ok(TransactionStatus::Missing),
        Err(err) => Err(anyhow::anyhow!(
            "Error fetching transaction status: {err:?}"
        )),
    }
}

fn build_provider(
    chain_kind: ChainKind,
    evm_config: &Evm,
) -> anyhow::Result<impl WalletProvider + Provider> {
    let private_key = config::get_private_key(chain_kind, None);
    let decoded_key = hex::decode(private_key).context("Failed to decode private key")?;
    let signing_key = alloy::signers::k256::ecdsa::SigningKey::from_slice(&decoded_key)
        .context("Invalid private key")?;
    let signer = alloy::signers::local::LocalSigner::from_signing_key(signing_key);
    let wallet = EthereumWallet::from(signer.clone());

    let provider = ProviderBuilder::new()
        .wallet(wallet.clone())
        .connect_http(evm_config.rpc_http_url.parse().context("Invalid RPC URL")?);

    Ok(provider)
}

fn sleep(check_interval_seconds: u64) -> Sleep {
    tokio::time::sleep(Duration::from_secs(check_interval_seconds))
}

#[allow(clippy::as_conversions)]
#[allow(clippy::cast_precision_loss)]
pub fn wei_to_gwei(wei: u128) -> f64 {
    (wei as f64) / 1_000_000_000.0
}
