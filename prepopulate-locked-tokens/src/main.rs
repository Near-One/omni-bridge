use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::Parser;
use near_api::AccountId;
use omni_types::{ChainKind, OmniAddress};
use serde_json::json;
use tokio::time::{Duration, sleep};

mod apply;
mod clients;
mod config;
mod solvency;
mod tokens;

use apply::Entry;
use config::Network;
use tokens::TokenInfo;

/// How many per-token tasks to run concurrently before pausing. Kept modest so we don't
/// burst a large number of simultaneous connections at one RPC host (which causes dropped
/// connections / "communication error"); read calls also retry transient transport errors.
const BATCH_SIZE: usize = 20;
const BATCH_SLEEP: Duration = Duration::from_secs(2);

/// Chains a bridged fungible token can be locked on and whose supply we can read.
/// Btc/Zcash are intentionally excluded: there is no fungible bridged representation
/// to read a supply from (and no tokens have those origins).
const DESTINATION_CHAINS: [ChainKind; 11] = [
    ChainKind::Near,
    ChainKind::Eth,
    ChainKind::Arb,
    ChainKind::Base,
    ChainKind::Bnb,
    ChainKind::Pol,
    ChainKind::HyperEvm,
    ChainKind::Abs,
    ChainKind::Sol,
    ChainKind::Fogo,
    ChainKind::Strk,
];

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Network profile: selects the default endpoints/addresses and labels the output.
    #[arg(long, value_enum, default_value = "testnet")]
    network: Network,
    /// Execute on-chain: print the preview, ask for confirmation, then send
    /// `set_locked_tokens`. Without this flag the tool runs in dry mode (print only).
    #[arg(long, alias = "live")]
    execute: bool,
    /// Path for the JSON artifact (defaults to `locked-tokens-<network>.json`).
    #[arg(long)]
    output_file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args = Args::parse();
    let config = config::Config::resolve(args.network)?;

    let tokens_api_url = config.tokens_api_url.clone();
    let output_file = args
        .output_file
        .clone()
        .unwrap_or_else(|| format!("locked-tokens-{}.json", args.network.label()));

    let near_client = Arc::new(clients::near::Client::new(
        config.omni_bridge_account_id.clone(),
        &config.near_rpc_url,
        config.near_api_key.as_deref(),
    )?);
    let clients = Arc::new(clients::Clients::new(Arc::clone(&near_client), &config)?);

    let tokens = tokens::fetch_tokens(&tokens_api_url)
        .await
        .with_context(|| format!("Failed to fetch tokens from {tokens_api_url}"))?;
    println!("Fetched {} tokens from {tokens_api_url}", tokens.len());
    let tokens_by_id: HashMap<AccountId, TokenInfo> = tokens
        .iter()
        .map(|token| (token.token_id.clone(), token.clone()))
        .collect();

    // Phase 1: read live supply per (token, destination chain).
    let (totals, supply_failures) = compute_totals(Arc::clone(&clients), tokens).await;

    // Phase 2: read the current on-chain locked amounts and build entries.
    let (entries, locked_failures) = build_entries(Arc::clone(&near_client), totals).await;
    let read_failures = supply_failures + locked_failures;

    write_artifact(&output_file, &entries)?;
    println!("Wrote {} entries to {output_file}", entries.len());

    print_preview(&entries);

    // Phase 3: solvency pre-check — Σ(routes) <= origin custody, for every token.
    let report = solvency::check(&clients, &config, &tokens_by_id, &entries).await;
    print_report(&report);

    if args.execute {
        if read_failures > 0 {
            bail!(
                "{read_failures} genuine RPC/data failure(s) during reads; refusing to write \
                 partial data on-chain. Fix the failing RPC(s) and re-run."
            );
        }
        if !report.is_clean() {
            bail!(
                "solvency check did not pass for all tokens \
                 ({} violation(s), {} unverifiable, {} read error(s)); aborting without writing.",
                report.violations.len(),
                report.unverifiable.len(),
                report.read_errors.len()
            );
        }
        println!("Solvency check passed for all tokens.");
        apply::run_live(near_client, &config.omni_bridge_account_id, &entries).await?;
    } else {
        if read_failures > 0 {
            println!(
                "\nWARNING: {read_failures} genuine read failure(s) — coverage is INCOMPLETE; \
                 the solvency result is not authoritative."
            );
        } else if report.is_clean() {
            println!("\nSolvency check passed for all tokens.");
        }
        let changed = entries.iter().filter(|entry| entry.changed()).count();
        println!(
            "Dry run. {changed} entr{} would change. Re-run with --execute to apply on-chain.",
            if changed == 1 { "y" } else { "ies" }
        );
    }

    Ok(())
}

/// Per-token supply task result: the (token, chain, locked) entries it produced and the
/// number of genuine failures (RPC/decode/decimal), distinct from "not deployed" skips.
struct SupplyTask {
    entries: Vec<(AccountId, ChainKind, u128)>,
    failures: u32,
}

async fn compute_totals(
    clients: Arc<clients::Clients>,
    tokens: Vec<TokenInfo>,
) -> (BTreeMap<(AccountId, ChainKind), u128>, u32) {
    let mut totals: BTreeMap<(AccountId, ChainKind), u128> = BTreeMap::new();
    let mut failures: u32 = 0;
    let mut handles = Vec::new();

    for token in tokens {
        let clients = Arc::clone(&clients);
        let handle = tokio::spawn(async move {
            let mut task = SupplyTask {
                entries: Vec::new(),
                failures: 0,
            };
            let token_address = OmniAddress::Near(token.token_id.clone());
            // locked_tokens is stored in origin-decimals units; a destination's
            // total_supply is in the normalized `decimals`. null origin_decimals => no scaling.
            let origin_decimals = token.origin_decimals.unwrap_or(token.decimals);

            for chain in DESTINATION_CHAINS {
                if chain == token.origin_chain {
                    continue;
                }
                let Some(client) = clients.client_for(chain) else {
                    continue;
                };

                match client.get_total_supply(token_address.clone()).await {
                    // Token has a representation on this chain.
                    // The NEAR representation of a foreign-origin token is minted in
                    // origin-decimals already (its NEP-141 decimals == origin_decimals),
                    // so it needs no scaling; every other representation is in normalized
                    // `decimals` and must be denormalized to the origin-decimals unit.
                    Ok(Some(supply)) => match locked_value(chain, supply, token.decimals, origin_decimals) {
                        Ok(locked) => {
                            println!(
                                "Token: {}, Origin: {:?}, Chain: {:?}, Supply: {} -> locked: {}",
                                token.token_id, token.origin_chain, chain, supply, locked
                            );
                            task.entries.push((token.token_id.clone(), chain, locked));
                        }
                        Err(err) => {
                            task.failures += 1;
                            eprintln!(
                                "FAILURE token {} on {:?}: decimal conversion: {}",
                                token.token_id, chain, err
                            );
                        }
                    },
                    // No representation on this chain — the expected, common case.
                    Ok(None) => {}
                    // Genuine RPC/decode failure: distinct from "not deployed".
                    Err(err) => {
                        task.failures += 1;
                        eprintln!(
                            "FAILURE token {} on {:?}: {:#}",
                            token.token_id, chain, err
                        );
                    }
                }
            }

            task
        });
        handles.push(handle);

        if handles.len() >= BATCH_SIZE {
            drain_supply_tasks(&mut handles, &mut totals, &mut failures).await;
            sleep(BATCH_SLEEP).await;
        }
    }

    drain_supply_tasks(&mut handles, &mut totals, &mut failures).await;
    (totals, failures)
}

/// Resilient like `build_entries`: a task panic (JoinError) counts as a failure and is
/// skipped rather than aborting the whole run (so dry mode still produces a preview).
async fn drain_supply_tasks(
    handles: &mut Vec<tokio::task::JoinHandle<SupplyTask>>,
    totals: &mut BTreeMap<(AccountId, ChainKind), u128>,
    failures: &mut u32,
) {
    for handle in handles.drain(..) {
        match handle.await {
            Ok(task) => {
                *failures += task.failures;
                for (token_id, chain, amount) in task.entries {
                    totals.insert((token_id, chain), amount);
                }
            }
            Err(err) => {
                *failures += 1;
                eprintln!("Token supply task join failed: {err}");
            }
        }
    }
}

/// Reads the current on-chain locked amount per (token, chain). Resilient: a failed
/// read logs, counts as a failure, and leaves `current = None` (the entry is still
/// produced) rather than aborting the whole run. Returns (entries, failure count).
async fn build_entries(
    near_client: Arc<clients::near::Client>,
    totals: BTreeMap<(AccountId, ChainKind), u128>,
) -> (Vec<Entry>, u32) {
    let items: Vec<((AccountId, ChainKind), u128)> = totals.into_iter().collect();
    let mut entries = Vec::with_capacity(items.len());
    let mut failures: u32 = 0;

    for chunk in items.chunks(BATCH_SIZE) {
        let mut handles = Vec::new();
        for ((token_id, chain), computed) in chunk.iter().cloned() {
            let near_client = Arc::clone(&near_client);
            handles.push(tokio::spawn(async move {
                match near_client.get_locked_tokens(chain, &token_id).await {
                    Ok(current) => (
                        Entry {
                            token_id,
                            chain,
                            computed,
                            current,
                        },
                        false,
                    ),
                    Err(err) => {
                        eprintln!(
                            "FAILURE reading current locked for {token_id} on {chain:?}: {err:#}"
                        );
                        (
                            Entry {
                                token_id,
                                chain,
                                computed,
                                current: None,
                            },
                            true,
                        )
                    }
                }
            }));
        }

        for handle in handles {
            match handle.await {
                Ok((entry, failed)) => {
                    if failed {
                        failures += 1;
                    }
                    entries.push(entry);
                }
                Err(err) => {
                    failures += 1;
                    eprintln!("Locked-token query task join failed: {err}");
                }
            }
        }
        sleep(BATCH_SLEEP).await;
    }

    (entries, failures)
}

/// The `locked_tokens` value (origin-decimals) for a destination's `total_supply`.
///
/// The NEAR representation of a foreign-origin token is already in origin-decimals (its
/// NEP-141 decimals equals the origin decimals, and the bridge mints `denormalize_amount`
/// to it), so no scaling. Every other representation is in normalized `decimals` and is
/// denormalized to origin-decimals.
fn locked_value(chain: ChainKind, supply: u128, decimals: u8, origin_decimals: u8) -> Result<u128> {
    if chain == ChainKind::Near {
        Ok(supply)
    } else {
        denormalize(supply, decimals, origin_decimals)
    }
}

/// Convert a normalized-`decimals` amount into the origin-decimals unit — mirroring the
/// contract's `denormalize_amount`: `supply * 10^(origin_decimals - decimals)`.
///
/// The contract invariant is `origin_decimals >= decimals`; a violation (bad input
/// data) is an error rather than a silently wrong value, as is a `u128` overflow.
fn denormalize(supply: u128, decimals: u8, origin_decimals: u8) -> Result<u128> {
    let diff = origin_decimals.checked_sub(decimals).with_context(|| {
        format!("origin_decimals ({origin_decimals}) < decimals ({decimals})")
    })?;
    let factor = 10u128
        .checked_pow(u32::from(diff))
        .context("decimal scaling factor overflows u128")?;
    supply
        .checked_mul(factor)
        .context("denormalized locked amount overflows u128")
}

fn write_artifact(path: &str, entries: &[Entry]) -> Result<()> {
    let output: Vec<_> = entries
        .iter()
        .map(|entry| {
            json!({
                "chain_kind": entry.chain,
                "token_id": entry.token_id,
                "amount": entry.computed.to_string(),
                "current_locked": entry.current.map(|value| value.to_string()),
            })
        })
        .collect();
    let serialized = serde_json::to_vec_pretty(&output).context("Failed to serialize output")?;
    fs::write(path, serialized).with_context(|| format!("Failed to write {path}"))?;
    Ok(())
}

fn print_preview(entries: &[Entry]) {
    println!("\nComputed locked tokens ({} entries):", entries.len());
    println!(
        "  {:<6} {:<52} {:>30} {:>30}",
        "CHAIN", "TOKEN_ID", "AMOUNT", "CURRENT_ON_CHAIN"
    );
    for entry in entries {
        let current = entry
            .current
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        let marker = if entry.changed() { "*" } else { " " };
        println!(
            "{marker} {:<6} {:<52} {:>30} {:>30}",
            entry.chain.as_ref(),
            entry.token_id.as_str(),
            entry.computed,
            current
        );
    }
    let changed = entries.iter().filter(|entry| entry.changed()).count();
    println!("({changed} marked * would change on-chain)");
}

fn print_report(report: &solvency::Report) {
    if !report.violations.is_empty() {
        println!(
            "\nSOLVENCY VIOLATIONS ({}) — sum of routes (minted) exceeds origin custody (backing):",
            report.violations.len()
        );
        for v in &report.violations {
            println!(
                "  {} (origin {:?}): routes={} > custody={}",
                v.token_id, v.origin_chain, v.routes_total, v.custody
            );
        }
    }
    if !report.unverifiable.is_empty() {
        println!(
            "\nUNVERIFIABLE ({}) — origin-chain backing could not be read:",
            report.unverifiable.len()
        );
        for (token_id, origin_chain) in &report.unverifiable {
            println!("  {token_id} (origin {origin_chain:?})");
        }
    }
    if !report.read_errors.is_empty() {
        println!("\nCUSTODY READ ERRORS ({}):", report.read_errors.len());
        for (token_id, err) in &report.read_errors {
            println!("  {token_id}: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{denormalize, locked_value};
    use omni_types::ChainKind;

    #[test]
    fn locked_value_does_not_denormalize_near_route() {
        // NEAR representation of a foreign-origin token is already in origin-decimals.
        assert_eq!(locked_value(ChainKind::Near, 42, 9, 18).unwrap(), 42);
    }

    #[test]
    fn locked_value_denormalizes_foreign_route() {
        // EVM/SVM/Starknet representations are in normalized `decimals`.
        assert_eq!(locked_value(ChainKind::Eth, 42, 9, 18).unwrap(), 42_000_000_000);
    }

    #[test]
    fn denormalize_is_identity_when_decimals_match() {
        assert_eq!(denormalize(1_500_000, 6, 6).unwrap(), 1_500_000);
    }

    #[test]
    fn denormalize_scales_up_to_origin_decimals() {
        // decimals=9, origin_decimals=18 => multiply by 10^9.
        assert_eq!(denormalize(42, 9, 18).unwrap(), 42_000_000_000);
    }

    #[test]
    fn denormalize_treats_null_origin_as_no_scaling() {
        // Caller passes origin_decimals = decimals when the API value is null.
        assert_eq!(denormalize(7, 6, 6).unwrap(), 7);
    }

    #[test]
    fn denormalize_rejects_origin_below_decimals() {
        assert!(denormalize(1, 18, 6).is_err());
    }

    #[test]
    fn denormalize_rejects_overflow() {
        assert!(denormalize(u128::MAX, 0, 2).is_err());
    }
}
