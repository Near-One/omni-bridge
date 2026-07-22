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
metadata object). Native SUI's token id is
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

Sui cannot create a currency at runtime (`create_currency` requires a
one-time witness, which only exists in a fresh package's `init`), so
unlike Aptos this is a two-transaction flow:

1. Copy [`token_template/`](token_template), rename the module + OTW
   struct, set `decimals = min(origin_decimals, 9)`, `symbol`, `name` to
   the values from the MPC-signed `MetadataPayload`, and publish it. The
   publisher receives the `TreasuryCap`, `CoinMetadata` and `UpgradeCap`.
2. Call `deploy_token<T>(state, signature, token, name, symbol, decimals,
   treasury_cap, upgrade_cap, coin_metadata)`. The bridge verifies the
   MPC signature and binds `T` to the NEAR token id after checking:
   - `TreasuryCap` total supply is zero,
   - the `UpgradeCap` controls `T`'s defining package at version 1 — it
     is then made immutable (one coin per package, forever),
   - `CoinMetadata` name/symbol equal the signed payload and decimals
     equal the clamped value.

The `DeployToken` event is then proven to NEAR (`bind_token`).

### Known residual risk (accepted design trade-off)

The MPC-signed `MetadataPayload` contains only
`(near_token_id, name, symbol, decimals)` — it *cannot* name the Sui coin
type, because package ids don't exist when NEAR signs. `deploy_token` is
deliberately permissionless (parity with the sibling chains), so a
front-runner watching NEAR for deploy signatures can bind their own
metadata-matching coin first. The binding checks make such a coin
functionally identical to an honest one, **except** a coin pre-created as
a *regulated* currency: the attacker would retain its `DenyCapV2` and
could later freeze transfers of that bridged token (griefing, not theft —
but the binding is permanent). Regulated-ness is not verifiable on-chain
today. Operational mitigations: relayers should submit `deploy_token`
promptly after the signature appears, and `PAUSE_DEPLOY_TOKEN` (0x04) can
gate the window.

## Native fees

`init_transfer` collects the optional `native_fee` as `Coin<SUI>` into
bridge custody. Custodied native fees back the wrapped-native SUI minted
to fee recipients on NEAR and can leave custody again through a regular
`fin_transfer<SUI>`.

## Testing

```sh
cd sui
sui move test
```

Coverage highlights: byte-exact borsh payload layouts, real secp256k1
signature vectors (generated offline; positive + negative), end-to-end
lock→unlock and deploy→mint→burn flows, nonce-bitmap word boundaries,
role/pause/version gates, deploy_token binding guards.

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
