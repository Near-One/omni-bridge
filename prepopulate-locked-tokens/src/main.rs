use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use omni_types::{ChainKind, OmniAddress};
use serde_json::json;
use tokio::time::{sleep, Duration};

mod clients;
mod config;
mod tokens;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(long, default_value = "tokens-testnet.txt")]
    tokens_file: String,
    #[arg(long, default_value = "locked-tokens.json")]
    output_file: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    const BATCH_SIZE: usize = 50;
    const BATCH_SLEEP: Duration = Duration::from_secs(2);

    dotenv::dotenv().ok();

    let args = Args::parse();
    let tokens = tokens::read_tokens(&args.tokens_file)?;

    let config = config::Config::from_env()?;
    let near_client = Arc::new(clients::near::Client::new(
        config.omni_bridge_account_id,
        &config.near_rpc_url,
    )?);
    let clients = clients::Clients::new(
        near_client.clone(),
        config.eth_rpc_url,
        config.base_rpc_url,
        config.arb_rpc_url,
        config.bnb_rpc_url,
        config.pol_rpc_url,
        config.solana_rpc_url,
    )?;

    let clients = Arc::new(clients);
    let mut totals: BTreeMap<(String, ChainKind), u128> = BTreeMap::new();
    let mut handles = Vec::new();
    for token in tokens {
        let clients = Arc::clone(&clients);
        let near_client = Arc::clone(&near_client);
        let handle = tokio::spawn(async move {
            let mut entries = Vec::new();
            let OmniAddress::Near(token_id) = near_client
                .get_bridged_token(&token, ChainKind::Near)
                .await?
            else {
                unreachable!("Unexpected address type");
            };

            let token_origin_chain = tokens::get_token_origin_chain(&token_id);
            for chain in [
                ChainKind::Near,
                ChainKind::Eth,
                ChainKind::Base,
                ChainKind::Arb,
                ChainKind::Bnb,
                ChainKind::Pol,
                ChainKind::Sol,
            ] {
                if chain == token_origin_chain {
                    continue;
                }

                let client: &dyn clients::Client = match chain {
                    ChainKind::Near => clients.near.as_ref(),
                    ChainKind::Eth => &clients.eth,
                    ChainKind::Base => &clients.base,
                    ChainKind::Arb => &clients.arb,
                    ChainKind::Bnb => &clients.bnb,
                    ChainKind::Pol => &clients.pol,
                    ChainKind::Sol => &clients.solana,
                    other => {
                        eprintln!("Unsupported chain encountered: {:?}", other);
                        continue;
                    }
                };

                let total_supply = match client.get_total_supply(token.clone()).await {
                    Ok(supply) => supply,
                    Err(err) => {
                        eprintln!(
                            "Failed to get total supply for token: {}, chain: {:?}, error: {}",
                            token, chain, err
                        );
                        continue;
                    }
                };
                entries.push((token_id.to_string(), chain, total_supply));

                println!(
                    "Token: {}, Origin Chain: {:?}, Checked Chain: {:?}, Total Supply: {}",
                    token, token_origin_chain, chain, total_supply
                );
            }

            Ok::<Vec<(String, ChainKind, u128)>, anyhow::Error>(entries)
        });
        handles.push(handle);

        if handles.len() >= BATCH_SIZE {
            for handle in handles.drain(..) {
                let entries = handle.await.context("Token task join failed")??;
                for (token, chain, amount) in entries {
                    totals.insert((token, chain), amount);
                }
            }
            sleep(BATCH_SLEEP).await;
        }
    }

    for handle in handles {
        let entries = handle.await.context("Token task join failed")??;
        for (token, chain, amount) in entries {
            totals.insert((token, chain), amount);
        }
    }

    let output: Vec<_> = totals
        .into_iter()
        .map(|((token, chain), amount)| {
            json!({
                "token": token,
                "destination_chain": chain.as_ref(),
                "amount": amount.to_string(),
            })
        })
        .collect();
    let json = serde_json::to_vec_pretty(&output).context("Failed to serialize output")?;
    fs::write(&args.output_file, json).context("Failed to write output file")?;

    Ok(())
}
