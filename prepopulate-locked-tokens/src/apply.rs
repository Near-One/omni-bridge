use anyhow::{Context, Result, anyhow};
use near_api::{AccountId, SecretKey, Signer};
use omni_types::ChainKind;
use serde::Serialize;
use std::io::{self, Write};
use std::sync::Arc;

use crate::clients::near;

/// Max args per `set_locked_tokens` transaction (gas-bounded; each tx uses max gas).
pub const APPLY_BATCH_SIZE: usize = 50;

/// One `(chain_kind, token_id, amount)` argument for `set_locked_tokens`.
/// `amount` is a decimal string so it deserializes into the contract's `U128`.
#[derive(Debug, Clone, Serialize)]
pub struct SetLockedTokenArg {
    pub chain_kind: ChainKind,
    pub token_id: AccountId,
    pub amount: String,
}

/// A computed locked-token value together with the current on-chain value.
pub struct Entry {
    pub token_id: AccountId,
    pub chain: ChainKind,
    pub computed: u128,
    pub current: Option<u128>,
}

impl Entry {
    /// Whether applying this entry would change on-chain state.
    pub fn changed(&self) -> bool {
        self.current != Some(self.computed)
    }

    fn to_arg(&self) -> SetLockedTokenArg {
        SetLockedTokenArg {
            chain_kind: self.chain,
            token_id: self.token_id.clone(),
            amount: self.computed.to_string(),
        }
    }
}

/// Live mode: confirm, then send `set_locked_tokens` in batches. Only entries that
/// differ from the current on-chain value are sent. Aborts on the first failed batch.
pub async fn run_live(
    near_client: Arc<near::Client>,
    bridge_account: &AccountId,
    entries: &[Entry],
) -> Result<()> {
    let to_set: Vec<SetLockedTokenArg> = entries
        .iter()
        .filter(|entry| entry.changed())
        .map(Entry::to_arg)
        .collect();
    let unchanged = entries.len() - to_set.len();

    if to_set.is_empty() {
        println!(
            "All {} entries already match on-chain state. Nothing to do.",
            entries.len()
        );
        return Ok(());
    }

    let signer_id: AccountId = env_required("NEAR_SIGNER_ACCOUNT_ID")?
        .parse()
        .context("Invalid NEAR_SIGNER_ACCOUNT_ID")?;
    let secret_key: SecretKey = env_required("NEAR_SIGNER_SECRET_KEY")?
        .parse()
        .map_err(|err| anyhow!("Invalid NEAR_SIGNER_SECRET_KEY: {err}"))?;
    let signer = Signer::from_secret_key(secret_key).context("Failed to build signer")?;

    let batches = to_set.len().div_ceil(APPLY_BATCH_SIZE);
    println!();
    println!("LIVE MODE");
    println!("  contract:       {bridge_account}");
    println!("  signer:         {signer_id} (must hold Role::TokenLockController or Role::DAO)");
    println!("  entries to set: {} ({unchanged} unchanged, skipped)", to_set.len());
    println!("  batches:        {batches} (up to {APPLY_BATCH_SIZE} entries each)");

    if !confirm("Proceed with sending set_locked_tokens? [y/N] ")? {
        println!("Aborted by user. No transactions sent.");
        return Ok(());
    }

    for (index, batch) in to_set.chunks(APPLY_BATCH_SIZE).enumerate() {
        print!("  batch {}/{} ({} entries)... ", index + 1, batches, batch.len());
        io::stdout().flush().ok();
        near_client
            .set_locked_tokens(signer_id.clone(), signer.clone(), batch)
            .await
            .with_context(|| {
                format!(
                    "batch {}/{} failed; {} batch(es) already applied (re-run to retry — \
                     set_locked_tokens is idempotent)",
                    index + 1,
                    batches,
                    index
                )
            })?;
        println!("ok");
    }

    println!("All {batches} batch(es) applied successfully.");
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read confirmation")?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Missing `{key}` env variable (required for --execute)"))
}
