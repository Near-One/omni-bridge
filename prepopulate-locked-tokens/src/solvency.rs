use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use near_api::AccountId;
use omni_types::{ChainKind, H160, H256, OmniAddress};
use solana_sdk::pubkey::Pubkey;

use crate::apply::Entry;
use crate::clients::{Clients, svm};
use crate::config::Config;
use crate::tokens::TokenInfo;

/// A token whose minted routes exceed its backing on the origin chain.
pub struct Violation {
    pub token_id: AccountId,
    pub origin_chain: ChainKind,
    /// Sum of the per-route locked values being set (origin-decimals units).
    pub routes_total: u128,
    /// The bridge's custody balance on the origin chain (origin-decimals units).
    pub custody: u128,
}

/// Outcome of the solvency pre-check. The check itself never aborts; per-token problems
/// are collected so dry mode can report every one. `is_clean()` is the gate for writing.
#[derive(Default)]
pub struct Report {
    /// Tokens where Σ(routes) > origin custody.
    pub violations: Vec<Violation>,
    /// Tokens whose origin custody can't be read (UTXO chains) — can't be verified.
    pub unverifiable: Vec<(AccountId, ChainKind)>,
    /// Tokens whose custody read failed (RPC/decode/missing config).
    pub read_errors: Vec<(AccountId, String)>,
}

impl Report {
    /// Safe to write only when every token reconciled: no violations, nothing
    /// unverifiable, no custody read errors.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty() && self.unverifiable.is_empty() && self.read_errors.is_empty()
    }
}

/// For each token, verify `Σ(route locked values) <= origin-chain custody`.
///
/// Both sides are in origin-decimals units (route values are already denormalized).
/// The policy is to verify every token before writing, so anything we can't confirm
/// (unreadable custody, a read error) is recorded and — via `Report::is_clean` — blocks
/// `--execute`, rather than silently passing.
pub async fn check(
    clients: &Clients,
    config: &Config,
    tokens_by_id: &HashMap<AccountId, TokenInfo>,
    entries: &[Entry],
) -> Report {
    // Sum the per-route locked values we would set, grouped by token.
    let mut routes_total: BTreeMap<AccountId, u128> = BTreeMap::new();
    for entry in entries {
        let slot = routes_total.entry(entry.token_id.clone()).or_default();
        *slot = slot.saturating_add(entry.computed);
    }

    let mut report = Report::default();
    for (token_id, routes) in routes_total {
        // Nothing minted on any route => Σ(routes) = 0 <= custody trivially. Skip the
        // custody read entirely: it adds no signal and would otherwise turn an unreadable
        // origin (e.g. a defunct token whose origin address isn't a live contract) into a
        // spurious read error, even though there's nothing to reconcile.
        if routes == 0 {
            continue;
        }
        let Some(token) = tokens_by_id.get(&token_id) else {
            report
                .read_errors
                .push((token_id, "missing token info".to_string()));
            continue;
        };

        match origin_custody(clients, config, token).await {
            // Verified: compare Σ(routes) against the backing.
            Ok(Some(custody)) => {
                if routes > custody {
                    report.violations.push(Violation {
                        token_id,
                        origin_chain: token.origin_chain,
                        routes_total: routes,
                        custody,
                    });
                }
            }
            // Custody not readable (UTXO origins): can't verify -> blocks writing.
            Ok(None) => report.unverifiable.push((token_id, token.origin_chain)),
            // Read failure: recorded, doesn't abort the rest of the check.
            Err(err) => report.read_errors.push((token_id, format!("{err:#}"))),
        }
    }

    report
}

/// Read the bridge's custody balance of `token` on its origin chain, in origin-decimals
/// units. `Ok(None)` means custody isn't readable for this origin (UTXO chains).
async fn origin_custody(
    clients: &Clients,
    config: &Config,
    token: &TokenInfo,
) -> Result<Option<u128>> {
    let origin = token.origin_chain;
    match origin {
        ChainKind::Near => Ok(Some(
            clients
                .near
                .ft_balance_of(&token.token_id, &config.omni_bridge_account_id)
                .await?,
        )),
        ChainKind::Btc | ChainKind::Zcash => Ok(None),
        evm_chain if evm_chain.is_evm_chain() => {
            let bridge = parse_h160(&bridge_id(config, evm_chain)?, evm_chain)?;
            let evm = clients
                .evm_client(evm_chain)
                .context("no EVM client for origin chain")?;
            let origin_address = resolve_origin_address(clients, &token.token_id, evm_chain).await?;
            let token_h160 = match_evm(evm_chain, origin_address)?;
            // The zero address marks a native coin; otherwise it's an ERC-20.
            if token_h160.0 == [0u8; 20] {
                Ok(Some(evm.native_balance(bridge).await?))
            } else {
                match evm.balance_of(token_h160.clone(), bridge).await {
                    Ok(balance) => Ok(Some(balance)),
                    // `balanceOf` reverts (returns `0x`) when the token contract doesn't
                    // exist. A non-existent contract holds nothing, so custody is a
                    // definitive 0 — surfacing under-backed routes as a real violation
                    // instead of an opaque read error. Only rescue when the address has no
                    // code; a contract that exists but failed the call is a genuine error.
                    Err(err) => {
                        if evm.is_contract(token_h160).await? {
                            Err(err)
                        } else {
                            Ok(Some(0))
                        }
                    }
                }
            }
        }
        ChainKind::Sol | ChainKind::Fogo => {
            let program = Pubkey::from_str(&bridge_id(config, origin)?)
                .with_context(|| format!("invalid {origin:?} bridge program id"))?;
            let svm_client = clients
                .svm_client(origin)
                .context("no SVM client for origin chain")?;
            let origin_address = resolve_origin_address(clients, &token.token_id, origin).await?;
            let mint_bytes = match origin_address {
                OmniAddress::Sol(addr) | OmniAddress::Fogo(addr) => addr.0,
                other => bail!("expected SVM origin address, got {other}"),
            };
            // All-zero mint marks native SOL; otherwise an SPL mint with a token vault.
            if mint_bytes == [0u8; 32] {
                let vault = svm::derive_sol_vault(&program);
                Ok(Some(svm_client.account_lamports(&vault).await?))
            } else {
                let vault = svm::derive_token_vault(&program, &Pubkey::new_from_array(mint_bytes));
                Ok(Some(svm_client.token_account_balance(&vault).await?))
            }
        }
        ChainKind::Strk => {
            let bridge = parse_h256(&bridge_id(config, ChainKind::Strk)?)?;
            let origin_address = resolve_origin_address(clients, &token.token_id, ChainKind::Strk).await?;
            let OmniAddress::Strk(contract) = origin_address else {
                bail!("expected Starknet origin address, got {origin_address}");
            };
            Ok(Some(clients.strk.balance_of(&contract, &bridge).await?))
        }
        // Unreachable: the `is_evm_chain()` guard above covers every remaining variant.
        other => bail!("unsupported origin chain for solvency check: {other:?}"),
    }
}

/// The origin-chain address of a token, via the contract's `get_bridged_token`.
async fn resolve_origin_address(
    clients: &Clients,
    token_id: &AccountId,
    origin: ChainKind,
) -> Result<OmniAddress> {
    clients
        .near
        .get_bridged_token(&OmniAddress::Near(token_id.clone()), origin)
        .await?
        .with_context(|| format!("token {token_id} has no address on its origin chain {origin:?}"))
}

fn match_evm(chain: ChainKind, address: OmniAddress) -> Result<H160> {
    match (chain, address) {
        (ChainKind::Eth, OmniAddress::Eth(a))
        | (ChainKind::Arb, OmniAddress::Arb(a))
        | (ChainKind::Base, OmniAddress::Base(a))
        | (ChainKind::Bnb, OmniAddress::Bnb(a))
        | (ChainKind::Pol, OmniAddress::Pol(a))
        | (ChainKind::HyperEvm, OmniAddress::HyperEvm(a))
        | (ChainKind::Abs, OmniAddress::Abs(a)) => Ok(a),
        (chain, other) => bail!("expected {chain:?} EVM origin address, got {other}"),
    }
}

fn bridge_id(config: &Config, chain: ChainKind) -> Result<String> {
    config
        .bridge_custody(chain)
        .map(str::to_string)
        .with_context(|| {
            format!(
                "solvency check needs a bridge custody address/program for origin chain \
                 {chain:?}; set the matching *_BRIDGE_ADDRESS / *_BRIDGE_PROGRAM env var"
            )
        })
}

fn parse_h160(value: &str, chain: ChainKind) -> Result<H160> {
    H160::from_str(value).map_err(|err| anyhow!("invalid {chain:?} bridge address {value}: {err:?}"))
}

fn parse_h256(value: &str) -> Result<H256> {
    H256::from_str(value).map_err(|err| anyhow!("invalid Starknet bridge address {value}: {err:?}"))
}
