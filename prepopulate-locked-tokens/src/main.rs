use std::collections::{BTreeMap, HashMap, HashSet};
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
    /// Additional `token_id`s to skip entirely (comma-separated); merged with SKIP_TOKENS.
    #[arg(long, value_delimiter = ',')]
    skip_tokens: Vec<String>,
    /// Cleanup mode: set every existing `(Near, token)` locked entry to 0 instead of running
    /// the normal seeding. The contract never reads or writes `(Near, *)` (a transfer's
    /// destination can never be Near, and the fin-to-near unlock keys on the foreign origin),
    /// so these are inert dead-writes left by seeding the NEAR leg of foreign-origin tokens;
    /// zeroing them is provably safe and just tidies the state. Honors `--execute`.
    #[arg(long)]
    zero_near_legs: bool,
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

    let mut tokens = tokens::fetch_tokens(&tokens_api_url)
        .await
        .with_context(|| format!("Failed to fetch tokens from {tokens_api_url}"))?;
    println!("Fetched {} tokens from {tokens_api_url}", tokens.len());

    // Cleanup mode: zero every existing (Near, token) entry. Independent of the skip-list and
    // the compute/solvency path — it only reads which (Near, *) keys are set and zeroes them.
    if args.zero_near_legs {
        let (entries, failures) = build_zero_near_legs(Arc::clone(&near_client), &tokens).await;
        let to_change = entries.iter().filter(|entry| entry.changed()).count();
        println!(
            "Found {} (Near, token) entries set on-chain; {to_change} are non-zero and would be \
             zeroed (the rest are already 0).",
            entries.len()
        );
        if failures > 0 {
            eprintln!(
                "WARNING: {failures} read failure(s) — some (Near, *) entries may be missing from \
                 this list; re-run to catch them (zeroing is idempotent)."
            );
        }
        write_artifact(&output_file, &entries)?;
        print_preview(&entries);
        if args.execute {
            apply::run_live(near_client, &config.omni_bridge_account_id, &entries).await?;
        } else {
            println!(
                "Dry run. {to_change} (Near, *) entr{} would be zeroed. Re-run with --execute to apply.",
                if to_change == 1 { "y" } else { "ies" }
            );
        }
        return Ok(());
    }

    // Skip-list: known-broken / unverifiable tokens (custody 0, non-contract origin,
    // `used_gas` in a view, …). Excluded from compute, solvency, and the write.
    let skip: HashSet<String> = config
        .skip_tokens
        .iter()
        .chain(args.skip_tokens.iter())
        .cloned()
        .collect();
    if !skip.is_empty() {
        let fetched: HashSet<&str> = tokens.iter().map(|token| token.token_id.as_str()).collect();
        for id in &skip {
            if !fetched.contains(id.as_str()) {
                eprintln!("WARNING: skip-list entry not present in token list: {id}");
            }
        }
        let before = tokens.len();
        tokens.retain(|token| !skip.contains(token.token_id.as_str()));
        println!(
            "Skip-list: excluded {} token(s); processing {}.",
            before - tokens.len(),
            tokens.len()
        );
    }

    // Re-anchor any token whose API origin has no bridge mapping to its true (NEAR) origin,
    // so both the compute pass and the solvency check below see the corrected origin.
    correct_origins(Arc::clone(&near_client), &mut tokens).await;

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
        let changed = entries.iter().filter(|entry| entry.write && entry.changed()).count();
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

/// Re-anchor tokens to the origin the **bridge** actually uses, which can differ from the
/// API's `origin_chain`. A foreign-origin token whose `get_bridged_token(origin)` is absent
/// has no foreign leg on the bridge: it's anchored on NEAR (the bridge can only lock it
/// there) and deployed outward — e.g. a NEAR Intents/defuse token like `starknet.omft.near`
/// (STRK), which the API labels `Strk` but the bridge locks on NEAR and minted on Solana.
///
/// Treating NEAR as the origin makes compute skip NEAR as a destination, read origin decimals
/// from `ft_metadata`, and solvency check NEAR custody (`ft_balance_of(bridge)`) — instead of
/// failing on a non-existent foreign leg. Tokens whose foreign mapping *exists* (even if the
/// foreign contract is gone, like defunct `pol-*`) are left as-is. A transient read error
/// leaves the origin unchanged (compute will surface it).
async fn correct_origins(near_client: Arc<clients::near::Client>, tokens: &mut [TokenInfo]) {
    let foreign: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.origin_chain != ChainKind::Near)
        .map(|(idx, _)| idx)
        .collect();

    let mut to_reanchor = Vec::new();
    for chunk in foreign.chunks(BATCH_SIZE) {
        let mut handles = Vec::new();
        for &idx in chunk {
            let near_client = Arc::clone(&near_client);
            let token_id = tokens[idx].token_id.clone();
            let address = OmniAddress::Near(token_id.clone());
            let origin = tokens[idx].origin_chain;
            handles.push(tokio::spawn(async move {
                // Re-anchor to NEAR only when BOTH hold, so the tool's origin matches the
                // contract's `get_token_origin_chain`:
                //   1. the bridge has no foreign leg for the API origin (`Ok(None)`), and
                //   2. the contract does not consider it a deployed token — `get_token_origin_chain`
                //      returns Near iff the token is not in `deployed_tokens`; a deployed token
                //      with a foreign-suggesting name would be inferred as foreign-origin.
                // `Ok(Some)` (a foreign leg exists) or any read error leaves the origin as-is.
                // A deployed token with no foreign mapping is left foreign on purpose: it then
                // surfaces as a compute failure (blocking --execute) rather than being seeded on
                // the wrong leg.
                let no_foreign_leg =
                    matches!(near_client.get_bridged_token(&address, origin).await, Ok(None));
                if !no_foreign_leg {
                    return None;
                }
                matches!(near_client.is_deployed_token(&token_id).await, Ok(false)).then_some(idx)
            }));
        }
        for handle in handles {
            if let Ok(Some(idx)) = handle.await {
                to_reanchor.push(idx);
            }
        }
        sleep(BATCH_SLEEP).await;
    }

    for idx in to_reanchor {
        let old = tokens[idx].origin_chain;
        tokens[idx].origin_chain = ChainKind::Near;
        eprintln!(
            "Re-anchoring {} origin {old:?} -> Near: no bridge mapping on the API origin chain \
             (the bridge anchors/locks it on NEAR and deployed it outward).",
            tokens[idx].token_id
        );
    }
}

/// The token's origin decimals — the unit `locked_tokens` is denominated in.
///
/// Prefers the bridge's recorded value (`get_token_decimals`), which is exactly what the
/// contract uses for `denormalize_amount` and needs no live origin-chain call, so it works
/// for origins a live `decimals()` read can't: a non-contract address, a native coin, or a
/// token whose `totalSupply` overflows. NEAR-origin tokens aren't keyed by their NEAR id in
/// that map, so they read `ft_metadata`; a foreign origin with no bridge record (rare)
/// falls back to a live read too.
///
/// `Ok(None)` means the token has no representation on its origin chain (a bridge mapping
/// gap) — the caller treats that as a failure. The Btc/Zcash case is handled by the caller
/// before this is reached.
async fn resolve_origin_decimals(
    clients: &clients::Clients,
    token: &TokenInfo,
) -> Result<Option<u8>> {
    let origin = token.origin_chain;
    let near_address = OmniAddress::Near(token.token_id.clone());

    // Foreign origin: resolve the origin-chain address, then read the bridge's recorded
    // decimals keyed by it. A `None` address is a genuine mapping gap (no representation).
    if origin != ChainKind::Near {
        match clients.near.get_bridged_token(&near_address, origin).await? {
            Some(origin_address) => {
                if let Some(decimals) = clients.near.get_token_decimals(&origin_address).await? {
                    return Ok(Some(decimals.origin_decimals));
                }
                // No bridge record (unexpected for a registered token): fall through to a
                // live decimals read below.
            }
            None => return Ok(None),
        }
    }

    // NEAR origin (read `ft_metadata`), or a foreign origin the bridge didn't record.
    match clients.client_for(origin) {
        Some(client) => client.get_decimals(near_address).await,
        None => Ok(None),
    }
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

            // Btc/Zcash origin: no queryable fungible representation (and no such tokens
            // exist) — a clean skip, not a failure.
            if clients.client_for(token.origin_chain).is_none() {
                return task;
            }

            // locked_tokens is denominated in the token's ORIGIN-chain decimals. Without
            // them we can't normalize any route, so a failure here skips the whole token.
            let origin_decimals = match resolve_origin_decimals(&clients, &token).await {
                Ok(Some(decimals)) => decimals,
                Ok(None) => {
                    task.failures += 1;
                    eprintln!(
                        "FAILURE token {}: not present on its origin chain {:?}",
                        token.token_id, token.origin_chain
                    );
                    return task;
                }
                Err(err) => {
                    task.failures += 1;
                    eprintln!(
                        "FAILURE token {}: reading origin decimals on {:?}: {:#}",
                        token.token_id, token.origin_chain, err
                    );
                    return task;
                }
            };

            for chain in DESTINATION_CHAINS {
                if chain == token.origin_chain {
                    continue;
                }
                let Some(client) = clients.client_for(chain) else {
                    continue;
                };

                match client.get_total_supply(token_address.clone()).await {
                    Ok(Some(supply)) => {
                        // The NEAR representation of a foreign-origin token is denominated
                        // in the token's origin decimals (its `ft_metadata` decimals can be
                        // unreliable for old factory tokens — sometimes 0); every other
                        // representation carries its own decimals.
                        let rep_decimals = if chain == ChainKind::Near {
                            origin_decimals
                        } else {
                            supply.decimals
                        };
                        match normalize(supply.amount, rep_decimals, origin_decimals) {
                            Ok(locked) => {
                                println!(
                                    "Token: {}, Origin: {:?}({}dp), Chain: {:?}, Supply: {} ({}dp) -> locked: {}",
                                    token.token_id,
                                    token.origin_chain,
                                    origin_decimals,
                                    chain,
                                    supply.amount,
                                    rep_decimals,
                                    locked
                                );
                                task.entries.push((token.token_id.clone(), chain, locked));
                            }
                            Err(err) => {
                                task.failures += 1;
                                eprintln!(
                                    "FAILURE token {} on {:?}: decimal normalization: {}",
                                    token.token_id, chain, err
                                );
                            }
                        }
                    }
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

/// Build zero-valued entries for every `(Near, token)` key currently set on-chain (used by
/// `--zero-near-legs`). Reads `get_locked_tokens(Near, token)` per fetched token; for each one
/// that is set, emits an `Entry` with `computed = 0` so `run_live` sends a zeroing
/// `set_locked_tokens` only for those not already 0. A token with no Near entry is skipped; a
/// read failure is logged and counted (the entry is simply omitted — re-running catches it,
/// since zeroing is idempotent). Returns (entries, failure count).
async fn build_zero_near_legs(
    near_client: Arc<clients::near::Client>,
    tokens: &[TokenInfo],
) -> (Vec<Entry>, u32) {
    let mut entries = Vec::new();
    let mut failures: u32 = 0;

    for chunk in tokens.chunks(BATCH_SIZE) {
        let mut handles = Vec::new();
        for token in chunk {
            let near_client = Arc::clone(&near_client);
            let token_id = token.token_id.clone();
            handles.push(tokio::spawn(async move {
                match near_client.get_locked_tokens(ChainKind::Near, &token_id).await {
                    Ok(Some(current)) => Ok(Some(Entry {
                        token_id,
                        chain: ChainKind::Near,
                        computed: 0,
                        current: Some(current),
                        // Cleanup mode deliberately writes the NEAR legs to 0.
                        write: true,
                    })),
                    Ok(None) => Ok(None),
                    Err(err) => Err(format!("locked(Near, {token_id}): {err:#}")),
                }
            }));
        }
        for handle in handles {
            match handle.await {
                Ok(Ok(Some(entry))) => entries.push(entry),
                Ok(Ok(None)) => {}
                Ok(Err(msg)) => {
                    failures += 1;
                    eprintln!("FAILURE reading {msg}");
                }
                Err(err) => {
                    failures += 1;
                    eprintln!("Zero-leg query task join failed: {err}");
                }
            }
        }
        sleep(BATCH_SLEEP).await;
    }

    (entries, failures)
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
                            // The NEAR leg of a foreign-origin token is inert (the contract
                            // never reads/writes `(Near, *)`): count it for solvency, never write it.
                            write: chain != ChainKind::Near,
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
                                write: chain != ChainKind::Near,
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

/// Normalize a representation's `supply` (in `rep_decimals`) into the origin-decimals unit
/// that `locked_tokens` is stored in: `supply * 10^(origin_decimals - rep_decimals)`.
///
/// Representations never carry MORE decimals than the origin (the bridge caps precision),
/// so `origin_decimals >= rep_decimals`; a violation (bad data) or a `u128` overflow is an
/// error rather than a silently wrong value.
fn normalize(supply: u128, rep_decimals: u8, origin_decimals: u8) -> Result<u128> {
    // Nothing minted on this route => locked is 0 regardless of decimals. Short-circuit so
    // a 0-supply representation never trips the decimals invariant below (e.g. a defunct
    // token the bridge records as 0 origin-decimals but whose NEAR rep reports 18).
    if supply == 0 {
        return Ok(0);
    }
    let diff = origin_decimals.checked_sub(rep_decimals).with_context(|| {
        format!("representation decimals ({rep_decimals}) exceed origin decimals ({origin_decimals})")
    })?;
    let factor = 10u128
        .checked_pow(u32::from(diff))
        .context("decimal scaling factor overflows u128")?;
    supply
        .checked_mul(factor)
        .context("normalized locked amount overflows u128")
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
                "written": entry.write,
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
        // `*` = will be written and changes state; `~` = NEAR leg, computed for solvency
        // only and never written; ` ` = already matches.
        let marker = if entry.write && entry.changed() {
            "*"
        } else if !entry.write {
            "~"
        } else {
            " "
        };
        println!(
            "{marker} {:<6} {:<52} {:>30} {:>30}",
            entry.chain.as_ref(),
            entry.token_id.as_str(),
            entry.computed,
            current
        );
    }
    let changed = entries.iter().filter(|entry| entry.write && entry.changed()).count();
    let solvency_only = entries.iter().filter(|entry| !entry.write).count();
    if solvency_only > 0 {
        println!(
            "({changed} marked * will be written on-chain; {solvency_only} '~' NEAR-leg rows are \
             counted for solvency only, never written)"
        );
    } else {
        println!("({changed} marked * would change on-chain)");
    }
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
    use super::normalize;

    #[test]
    fn normalize_is_identity_when_decimals_match() {
        // e.g. an Eth-decimals (18) representation of an 18-decimals-origin token.
        assert_eq!(normalize(1_100_000_000_000_000_000, 18, 18).unwrap(), 1_100_000_000_000_000_000);
    }

    #[test]
    fn normalize_scales_solana_rep_up_to_origin() {
        // Solana rep (9dp) of an 18dp-origin token => multiply by 10^9.
        assert_eq!(normalize(42, 9, 18).unwrap(), 42_000_000_000);
    }

    #[test]
    fn normalize_scales_strk_rep_for_24dp_origin() {
        // wNEAR (24dp origin), Starknet rep (18dp) => x10^6 (matches on-chain value).
        assert_eq!(normalize(1_999_999_999_999_999, 18, 24).unwrap(), 1_999_999_999_999_999_000_000);
    }

    #[test]
    fn normalize_rejects_rep_decimals_above_origin() {
        assert!(normalize(1, 18, 6).is_err());
    }

    #[test]
    fn normalize_zero_supply_is_zero_even_when_rep_exceeds_origin() {
        // A defunct token recorded as 0 origin-decimals whose NEAR rep reports 18dp, with
        // 0 supply: locked is 0, not a decimals-invariant error.
        assert_eq!(normalize(0, 18, 0).unwrap(), 0);
    }

    #[test]
    fn normalize_rejects_overflow() {
        assert!(normalize(u128::MAX, 0, 2).is_err());
    }
}
