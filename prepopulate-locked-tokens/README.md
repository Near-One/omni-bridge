# Prepopulate locked tokens

Seeds the omni-bridge `locked_tokens` guard with its initial per-`(chain, token)`
values, computed from the current bridged supply on each chain.

## Why this is needed

The bridge's locking guard (`near/omni-bridge/src/token_lock.rs`) is **opt-in per
`(destination_chain, token_id)`**: `lock`/`unlock` only enforce a limit once an entry
exists. Until a pair is seeded via `set_locked_tokens`, unlocks on it are unguarded.
After a contract migration this tool computes and (optionally) applies those initial
values.

For a destination chain, the locked amount is the current total supply of the token's
bridged representation on that chain (the origin chain is skipped), **converted into
origin-decimals units**. The contract locks `denormalize_amount(transfer.amount)`, so
`locked_tokens` is denominated in the token's `origin_decimals`, while a destination's
`total_supply` is in that representation's own decimals. The tool applies the same
conversion the contract does: `locked = total_supply * 10^(origin_decimals - rep_decimals)`.

Both inputs are read from the chain, not the API:

- **`rep_decimals`** — each representation's actual decimals, read per chain (EVM
  `decimals()`, SPL mint decimals, Starknet `decimals()`, NEAR `ft_metadata`), since they
  differ per chain (EVM 18, Solana ~9, …). A representation with **0 supply** contributes
  `locked = 0` regardless of decimals.
- **`origin_decimals`** — read from the bridge's own `get_token_decimals(origin_address)`
  record rather than the live origin chain. That record is exactly what the contract uses
  for `denormalize_amount`, so it matches on-chain math precisely, and it stays readable
  for origins a live `decimals()` call can't handle (a non-contract/defunct address that
  returns `0x`, a native coin, or a token whose `totalSupply` overflows). NEAR-origin
  tokens aren't keyed in that map, so their origin decimals come from `ft_metadata`.

## Where the token list comes from

The token list is fetched from the bridge API (no more MongoDB queries or static files):

- testnet: `https://testnet.api.bridge.nearone.org/api/v3/tokens`
- mainnet: `https://mainnet.api.bridge.nearone.org/api/v3/tokens`

Each entry provides the NEAR `token_id` and the authoritative `origin_chain`. Override
the URL with `TOKENS_API_URL` if needed; otherwise it is derived from `--network`.

## Configuration

All settings — RPC URLs, bridge custody addresses, the tokens API URL, and the locker
account — have canonical public defaults baked in per `--network`, so the tool runs with
**no env vars**. Set a variable (see `example.env`) only to override a default. The only
env vars normally needed are a NEAR API key (to avoid rate limits) and the signer for
live mode (`--execute`).

The public NEAR RPC rate-limits (429) under a full run's call volume, so set a fastnear
key via **`NEAR_API_KEY`** — it is sent as an `Authorization: Bearer` header. Do **not**
put the key in `NEAR_RPC_URL`: a query-string key (`?apiKey=…`) is corrupted by near_api's
RPC client (the JSON-RPC path is appended after the query) and fails with `401`.

Supported destination chains: NEAR, Eth, Arb, Base, Bnb, Pol, HyperEvm (`hlevm`), Abs,
Sol, Fogo, and Strk (Starknet). Btc/Zcash are not queried (no fungible bridged
representation).

## Usage

Dry mode (default) — reads supplies, writes `locked-tokens-<network>.json`, and prints a
preview of the computed values and how they differ from the current on-chain state. Sends
nothing:

```bash
cargo run -- --network mainnet
```

Live mode — same computation, then prints the preview, asks for confirmation, and sends
`set_locked_tokens` in batches:

```bash
cargo run -- --network mainnet --execute
```

Live mode additionally requires a signer in the environment:

```env
NEAR_SIGNER_ACCOUNT_ID=token-lock-controller.near
NEAR_SIGNER_SECRET_KEY=ed25519:...
```

The signer account must hold **`Role::TokenLockController`** (or `Role::DAO`) on the
bridge contract — that is the role `set_locked_tokens` requires. `get_locked_tokens`
(used for the dry-mode diff) is a public view and needs no role.

Only entries that differ from the current on-chain value are sent; `set_locked_tokens` is
an idempotent overwrite, so a run that aborts part-way can simply be re-run.

## Solvency pre-check

Before any write, the tool verifies for **every** token that the sum of its routes'
minted supply does not exceed the bridge's backing on the token's origin chain
(e.g. Σ wNEAR minted across all chains ≤ wNEAR the bridge holds on NEAR). The backing
is read per origin:

| Origin | Backing read |
|---|---|
| NEAR | `ft_balance_of(bridge)` on the token (uses `OMNI_BRIDGE_ACCOUNT_ID`) |
| EVM (ERC-20) | `balanceOf(bridge)` on the origin token |
| EVM (native, `0x0`) | the bridge's native balance |
| Solana/Fogo (SPL) | the `[b"vault", mint]` token-vault PDA balance |
| Solana/Fogo (native) | the `[b"sol_vault"]` PDA balance |
| Starknet | `balance_of(bridge)` on the origin Cairo ERC-20 |

Set the bridge custody address (EVM/Starknet) or program id (SVM) for each foreign origin
chain that has tokens via `*_BRIDGE_ADDRESS` / `*_BRIDGE_PROGRAM` (see `example.env`). If
any token fails the check — or any backing can't be read — the **whole run aborts** and
nothing is written. In dry mode the violations are reported instead.

A run also aborts before writing if any **genuine RPC/data read failure** occurred (as
opposed to the routine "token not deployed on this chain" case, which is a clean skip), so
a partially-failed read can never be mistaken for complete coverage.

## Skip-list

Some tokens can't be reconciled — broken/legacy ones with custody 0, or an
`ft_metadata`/`ft_balance_of` that calls `used_gas` (forbidden in a view). List their
`token_id`s in `SKIP_TOKENS` (comma-separated) or `--skip-tokens` to exclude them entirely
(compute, solvency, and the write). `example.env` seeds it with the omni-bridge-monitor's
known-bad set; add any token a dry run reports as a failure or an unexplained solvency
violation. Excluded tokens are logged (never silently dropped).

How the tool handles the awkward cases:

- **Defunct/non-contract origins with no supply** (most `pol-*.omdep.near` whose origin
  address has no code) are handled automatically: origin decimals come from the bridge
  record, supply is 0, and the solvency check skips zero-route tokens. They compute to `0`
  cleanly — no skip needed.
- **Defunct/non-contract origins *with* supply** (e.g. a `pol-*` whose Pol address has no
  code but whose NEAR rep still has a small balance) are reported as a **solvency
  violation**: a contract that doesn't exist holds nothing, so custody is a definitive `0`,
  and `Σ(routes) > 0` means the NEAR supply is unbacked. This is surfaced as a real
  violation (not an opaque read error), so the solvency result stays authoritative. Skip
  them — they're orphaned/unbacked supply (and can't be drained, since the origin contract
  is gone) — or investigate the orphaned supply.
- **API origin with no bridge leg** — a token whose `get_bridged_token(origin)` returns
  `null` for its API-declared `origin_chain` (no foreign address at all). The API's
  `origin_chain` is metadata that can differ from where the bridge actually anchors the
  token; if there's no foreign leg, the bridge can only **lock it on NEAR** and deploy it
  outward (e.g. a NEAR Intents/defuse token like `starknet.omft.near` (STRK), locked on NEAR
  and minted on Solana). The tool **re-anchors these to a NEAR origin automatically**: NEAR
  is skipped as a destination, origin decimals come from `ft_metadata`, the outward routes
  (e.g. Solana) are seeded, and solvency verifies them against NEAR custody
  (`ft_balance_of(bridge)`). No skip-list entry is needed, and a genuinely under-backed one
  would still surface as a solvency violation rather than being silently seeded.
