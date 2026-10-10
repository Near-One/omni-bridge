# Omni Bridge — Sui

Sui side of the NEAR Omni Bridge. Enables token transfers between Sui and
other chains via NEAR Protocol (Sui ↔ NEAR ↔ other chain). Mirrors the
[Aptos](../aptos) and [Starknet](../starknet) implementations.

## Trust model

- **Sui → NEAR** (outbound): the contract emits `InitTransfer` /
  `LogMetadata` / `FinTransfer` / `DeployToken` events; the NEAR MPC
  network reads them from Sui full nodes (`verify_foreign_transaction`)
  and the NEAR-side `mpc-omni-prover` verifies the MPC response. No
  Wormhole, no light client.
- **NEAR → Sui** (inbound): `fin_transfer` / `deploy_token` verify an
  Ethereum-style secp256k1 signature produced by the NEAR MPC over a
  borsh-encoded payload, recovered against the configured
  `near_bridge_derived_address` (20 bytes, key path `bridge-1`).

## Token identity

Sui coins are *types* (`Coin<T>`), not addresses. The wire-format token id
— what `OmniAddress::Sui` carries on NEAR, what events emit as
`token_address`, and what the signed `TransferMessagePayload` contains —
is:

```
keccak256(canonical_type_string(T))
```

where the canonical type string is the on-chain `std::type_name` form:
64-char lowercase hex defining-package id, **no `0x` prefix**,
`::module::NAME` (e.g. `0000…0002::sui::SUI`). Events also carry the type
string in a `coin_type` field, and the bridge keeps an on-chain
`token_registry` (id → type) so relayers/indexers can resolve ids without
external state. Sui-native coins are onboarded with `log_metadata<T>`
(classic `CoinMetadata<T>`) or `log_metadata_registry<T>` (coins under
the newer `coin_registry` Currency standard that may have no legacy
metadata object). Bridge-deployed coins always use `coin_registry`.
Native SUI's token id is
`keccak256(b"0000000000000000000000000000000000000000000000000000000000000002::sui::SUI")`
= `0x669638…df700c`.

## Deployment

1. `sui client publish` — `init` creates the shared `BridgeState` with the
   publisher holding the `Admin` / `Pauser` / `MetadataAdmin` roles.
2. `initialize(state, near_bridge_derived_address, chain_id)` — one-shot,
   Admin-gated. `chain_id` is the `ChainKind::Sui` discriminant on NEAR
   (expected **14** — must be reserved with the omni-bridge maintainers
   before mainnet deployment). Every bridge operation aborts until this
   has run.
3. Guard the package `UpgradeCap` (multisig) — it is the real root of
   trust for upgrades. The shared state carries a `version` gate +
   `migrate` entry point for the upgrade flow.

## Deploying a bridged token (NEAR-originated token on Sui)

Sui cannot create a currency at runtime (`new_currency_with_otw` requires
a one-time witness, which only exists in a fresh package's `init`), so
unlike Aptos this is a two-transaction, two-method flow — the same shape
as Wormhole's `prepare_registration` / `complete_registration`:

1. **Publish.** Point the bridge dependency at the live deployment first —
   `sui/Move.toml` needs *both* `published-at` and
   `[addresses] omni_bridge` set to the bridge's package id, since the
   named address defaults to `0x0` and is what the template links against.
   Then copy [`token_template/`](token_template) and set
   `DECIMALS = min(origin_decimals, 9)`, `SYMBOL` and `NAME` to the values
   from the MPC-signed `MetadataPayload` — all three are checked against
   the signature, so a stock or mismatched value wastes the package. Do
   **not** rename the module or the `TOKEN` struct. Its `init` calls
   `omni_bridge::prepare_token(otw, decimals, symbol, name, ctx)`, which
   creates the currency via `coin_registry::new_currency_with_otw` and
   returns a `TokenSetup<T>` to the publisher.
2. **Bind.** Call `deploy_token<T>(state, setup, upgrade_cap, signature,
   token, name, symbol, decimals)`. The bridge verifies the MPC signature
   and binds `T` to the NEAR token id after checking:
   - `setup.version` equals the package `VERSION`,
   - the `UpgradeCap` controls `T`'s defining package at version 1 — it is
     then made immutable (one coin per package, forever),
   - `T` is `<package>::token::TOKEN`,
   - the recorded name/symbol equal the signed payload and decimals equal
     the clamped value.

The `DeployToken` event is then proven to NEAR (`bind_token`).

Afterwards anyone may call `coin_registry::finalize_registration<T>` to
promote the coin's `Currency<T>` from the `CoinRegistry` address (`0xc`)
to its derived shared address. That is a prerequisite for
`set_token_metadata` and for wallets to resolve the coin from the registry;
it is permissionless and can happen at any time.

### Why the currency is created by the bridge, not the template

The MPC-signed `MetadataPayload` contains only
`(near_token_id, name, symbol, decimals)` — it *cannot* name the Sui coin
type, because package ids don't exist when NEAR signs. A `deploy_token`
that merely *inspected* a caller-supplied `TreasuryCap` could therefore
only reject the problems it thought to enumerate, and one was not
enumerable: a coin pre-created with
`coin::create_regulated_currency_v2` / `coin_registry::make_regulated`
hands its creator a `DenyCapV2` that can freeze every transfer of that
coin forever. It is invisible to any metadata check and cannot be ruled
out on-chain after the fact.

Creating the currency inside `prepare_token` closes that category
outright:

- `make_regulated` needs `&mut CurrencyInitializer<T>`, which never leaves
  `prepare_token`, so **no `DenyCapV2<T>` can ever exist**;
- the one-time witness is consumed exactly once, so a publisher who spends
  it on their own regulated currency gets no `TokenSetup` and can never
  reach `deploy_token`;
- `coin_registry::new_currency<T>` — the non-OTW constructor, which anyone
  may call for any unclaimed type — requires `T: key`, and a one-time
  witness has only `drop`, so the type cannot be squatted in the window
  between publish and deploy either.

`deploy_token` stays permissionless (parity with the sibling chains), so a
front-runner can still win the race — but doing so now just produces a
correct token: metadata equal to the signed payload, a canonical
`::token::TOKEN` type string, every capability in bridge custody, and no
deny capability in existence. The winner keeps nothing. `PAUSE_DEPLOY_TOKEN`
(0x04) remains available if you want to gate the window anyway.

Residual, accepted: whoever wins the race chooses the coin's *package id*
(never predictable in any design), and the NEAR token id is bound
permanently — there is no unbind, so a hostile bind is still a denial of
service for that token on this deployment.

A `TokenSetup` that can never be bound — mismatched metadata, or a bridge
upgrade that bumped `VERSION` before the bind — is not stranded:
`cancel_token_setup` returns its `TreasuryCap` and `MetadataCap` to the
owner and permanently retires that coin type. Note this is what a `VERSION`
bump does to pre-published deploy inventory, so re-publish it after an
upgrade.

### Regenerating test signature vectors

The test suite pins real secp256k1 signatures. Regenerate them with:

```sh
python3 tests/vectors/gen_signatures.py   # needs pycryptodome
```

The script mirrors the Borsh encoders in `sources/bridge_types.move` and
self-checks every signature by recovering it. Its `PRIV` is the well-known
go-ethereum test key; its address is what `derived_address()` in
`tests/omni_bridge_tests.move` must contain.

## Native fees

`init_transfer` collects the optional `native_fee` as `Coin<SUI>` into
bridge custody. Custodied native fees back the wrapped-native SUI minted
to fee recipients on NEAR and can leave custody again through a regular
`fin_transfer<SUI>`.

## Testing

```sh
cd sui && sui move test                  # 91 tests
cd sui/token_template && sui move build
make fmt-sui                             # prettier-move check (CI-enforced)
```

Formatting uses `sui move format` (prettier-move; install with
`npm i -g prettier @mysten/prettier-plugin-move`). `make fmt-sui-fix`
rewrites in place. Signature/byte fixtures are `x"…"` hex literals, which
the formatter leaves intact.

Coverage highlights: byte-exact borsh payload layouts, real secp256k1
signature vectors (generated offline; positive + negative), end-to-end
lock→unlock and prepare→deploy→mint→burn flows, nonce-bitmap word
boundaries, role/pause/version gates, `deploy_token` binding guards
(canonical type, upgrade cap, setup version, metadata equality), an
assertion that a prepared currency is unregulated with no deny cap, and
negative cases pinned to exact abort codes — malformed signature (native
ecrecover), valid signature from a non-bridge signer, non-ASCII symbol,
and the sub-clamp (6-decimal) deploy path.

## NEAR-side status

Mirroring the Aptos rollout (PRs #626 / #629):

- **Done**: `ChainKind::Sui` (= 14) + `OmniAddress::Sui(H256)` wiring in
  `omni-types` (`new_zero`, `new_from_slice`, `get_token_prefix` →
  `hashed_token_prefix("sui", …)`, `get_native_token_address` → the
  keccak constant above), the origin-chain token-prefix match in
  `omni-bridge`, and the enum-stability tests.

Remaining follow-ups:

- `near/omni-types/src/sui/events.rs` parsers — blocked on near/mpc
  defining the Sui read support (`SuiRpcRequest` / `SuiExtractedValue` /
  `SuiFinality`), which does not exist yet as of 2026-07.
  The emitter for the factory check should be the event type's
  **defining package id** parsed from the event type tag (stable across
  package upgrades), analogous to the Aptos type-tag-address rule.
- `MpcFinality::Sui` + dispatch in `mpc-omni-prover`.
- DAO calls: `add_factory(OmniAddress::Sui(...))`, `add_prover`,
  `add_token_deployer`, `deploy_native_token` for wrapped SUI.
