# OmniBridge Sui Contract

## Overview
Cross-chain bridge for Sui, enabling token transfers between Sui and other
chains via NEAR Protocol. Mirrors the Aptos, Starknet and EVM
implementations in this repo: see
[aptos/sources/omni_bridge.move](../aptos/sources/omni_bridge.move),
[starknet/src/omni_bridge.cairo](../starknet/src/omni_bridge.cairo) and
[evm/src/omni-bridge/contracts/OmniBridge.sol](../evm/src/omni-bridge/contracts/OmniBridge.sol).

## Architecture
- **NEAR-centric**: all transfers route through NEAR (Sui ↔ NEAR ↔ other
  chain).
- **Security**: NEAR→Sui messages carry Ethereum-style ECDSA signatures by
  the NEAR MPC, verified in `utils::verify_eth_signature` against
  `near_bridge_derived_address`. Sui→NEAR proofs are MPC reads of the
  events emitted here (`mpc-omni-prover` on NEAR) — no Wormhole, no light
  client.
- **Token identity**: Sui coins are types, not addresses. The 32-byte
  wire-format token id is `keccak256(canonical type string of T)` in the
  `std::type_name::with_defining_ids` form (64 lowercase hex chars, no
  `0x`, `::module::NAME`). `OmniAddress::Sui` on NEAR is `H256`, exactly
  like Aptos/Starknet. Events additionally carry the type string
  (`coin_type`) and the shared state keeps a `token_registry`
  (id → TypeName) reverse map.
- **Token model**: bridged coins (mint/burn via `TreasuryCap` custody) or
  native coins (lock/unlock in a `Bag` of `Balance<T>` keyed by
  `TypeName`). Bridge-token status == presence of the type in the
  `treasuries` ObjectBag — one source of truth.
- **Shared-object state**: one `BridgeState` shared object created in
  `init` at publish. Sui `init` cannot take parameters, so the MPC signer
  and chain id are set by a one-shot Admin `initialize` call; every bridge
  operation aborts with `E_NOT_INITIALIZED` until then.
- **Upgrade safety**: `BridgeState.version` is asserted by every entry
  point (`E_WRONG_VERSION`); upgrades bump `VERSION` and ship an
  Admin-gated `migrate`. Old package versions keep running on Sui — the
  version gate stops them from touching state.
- **Role-based access control**: `roles: Table<u8, vector<address>>`
  checked against `ctx.sender()`, same discriminants and semantics as
  Aptos (`ROLE_ADMIN = 0`, `ROLE_PAUSER = 1`, `ROLE_METADATA_ADMIN = 2`;
  grant/revoke by Admin; last-admin guard).

## Module Layout

| Module | Purpose |
|--------|---------|
| `omni_bridge::omni_bridge` | Main contract: init/initialize, deploy_token, init_transfer, fin_transfer, log_metadata (+ `_registry` variant), set_token_metadata, roles, pause, migrate, events, views |
| `omni_bridge::bridge_types` | Payload structs (`MetadataPayload`, `TransferMessagePayload`) and their Borsh encoders |
| `omni_bridge::borsh` | Borsh sequence encoders (u32-LE length prefix). Fixed-width integers and addresses use `std::bcs::to_bytes` directly at call sites (BCS == Borsh for those types) |
| `omni_bridge::utils` | `verify_eth_signature`, `normalize_decimals` (clamp 9), `coin_type_string<T>`, `token_address<T>` (keccak id), `type_package_address<T>` |
| `token_template::template_coin` | Separate per-token package template for `deploy_token` (one-time-witness constraint) |

## Core Functions

| Function | Purpose | Access |
|----------|---------|--------|
| `initialize` | One-shot: set MPC signer address + chain id | Admin |
| `init_transfer<T>` | Send tokens from Sui: burns (bridged) or locks (native), collects optional `Coin<SUI>` native fee | Public |
| `fin_transfer<T>` | Receive tokens: verifies MPC signature, marks `destination_nonce`, mints or unlocks to `recipient` | Public |
| `deploy_token<T>` | Bind a pre-published coin to a signed MetadataPayload; takes `TreasuryCap` + `UpgradeCap` (frozen) + `CoinMetadata` | Public (signature-authorized) |
| `log_metadata<T>` / `log_metadata_registry<T>` | Emit `LogMetadata` for an existing coin (classic `CoinMetadata` / new `coin_registry::Currency`) | Public |
| `set_token_metadata<T>` | Update `description` / `icon_url` on a bridge-deployed coin | MetadataAdmin |
| `set_near_bridge_derived_address` / `set_chain_id` | Correct the MPC signer address / chain id after `initialize` (both baked into the signed preimage, so both are recoverable) | Admin |
| `set_pause_flags` / `pause_all` | Pause bitmap (`0x01` init, `0x02` fin, `0x04` deploy) | Admin / Pauser |
| `grant_role` / `revoke_role` | Role management (last-admin guard) | Admin |
| `migrate` | Bump shared-object version after a package upgrade | Admin |
| Views | `is_transfer_finalised`, `get_token_address`, `get_coin_type`, `is_bridge_token<T>`, `locked_balance<T>`, `current_origin_nonce`, `pause_flags`, `chain_id`, `role_holders`, `has_role`, `all_roles` | — |

## Borsh / signature encoding

Payloads are byte-identical to the Aptos/Starknet layout; the 32-byte
`token_address` slot carries the keccak type id and `recipient` a native
32-byte Sui address. The destination chain id byte is interleaved before
both (OmniAddress enum tag) and bound into the signed hash, not the
payload. `fee_recipient` is a tagged Borsh `Option<String>`; `message` is
UNTAGGED (empty ⇒ zero bytes; else u32-LE length + bytes).

**Critical Sui difference**: `ecdsa_k1::secp256k1_ecrecover(sig, msg, 0)`
takes the RAW message and hashes it internally with keccak256 — pass the
borsh payload itself, never a digest (a naive Aptos port double-hashes and
always fails). Signature is 65 bytes `r||s||v`; NEAR emits
`v = recovery_id + 27`, normalized to {0,1} before the native call.
Ethereum address = last 20 bytes of `keccak256(decompressed_pubkey[1..65])`.

## Important Notes

### Design Decisions
1. **`deploy_token` cannot create the coin** (one-time-witness rule) — it
   binds a pre-published `TreasuryCap<T>`. Binding checks: zero supply,
   version-1 `UpgradeCap` for `T`'s defining package (then
   `make_immutable`), `CoinMetadata` equality with the signed payload.
   Front-running with a regulated coin (retained `DenyCapV2`) remains
   possible and is a documented, accepted griefing risk (see README) —
   regulated-ness is not on-chain-verifiable.
2. **Decimals clamp = 9** (SUI convention; Aptos uses 8). Sui `Coin`
   amounts are u64. NEAR does all decimal scaling; the signed amount
   arrives pre-scaled — this side only bounds u128 → u64.
3. **`init_transfer` takes an exact `Coin<T>`** (its full value is the
   amount) — PTBs make exact splitting trivial client-side; no refund
   path. `fee < amount` enforced; fee is bookkeeping inside the amount.
4. **`CoinMetadata` is surrendered** to the bridge in `deploy_token`
   (ObjectBag) so `set_token_metadata` can mutate it later — template
   coins must NOT freeze their metadata.
5. **Chain id is a parameter** (`initialize`), expected to be
   `ChainKind::Sui = 14`; must be reserved with maintainers before
   deployment. It is interleaved as the OmniAddress tag byte in every
   signed transfer payload, so a wrong value silently rejects all inbound
   transfers — `initialize` rejects `0`, and `set_chain_id` (admin) can
   correct a wrong non-zero value so a misconfig is never a permanent
   brick.
6. **Native SUI wire id** =
   `keccak256(b"0000…0002::sui::SUI")` — needed for NEAR's
   `get_native_token_address`.

### Security Invariants
- **No replay**: `destination_nonce` checked + marked used *before*
  signature verification and token movement in `fin_transfer`;
  `origin_nonce` incremented at the top of `init_transfer`.
- **Signature binds the coin type**: `fin_transfer<T>` derives the payload
  `token_address` from `T` itself — a wrong type argument reconstructs
  different bytes and fails recovery.
- **No token release without signature**: mint/unlock happens only after
  `verify_eth_signature`.
- **Version gate on every state-touching entry point**; the four bridge
  operations (init/fin transfer, deploy_token, log_metadata) additionally
  require `initialize` to have run, as do the `set_near_bridge_derived_address`
  / `set_chain_id` config setters. Role/pause admin entries are
  version-gated only, and `migrate` is role-gated only (it must run while
  the version is stale).
- **Custody cannot be moved by the deployer**: state is a shared object;
  only module code touches the `Bag`/`ObjectBag` fields. The package
  `UpgradeCap` is the root of trust — keep it in a multisig.

### Bitmap layout
`completed_transfers: Table<u64, u128>` packs nonces 128-per-slot
(`slot = nonce / 128`, `bit = nonce % 128`) — identical to Aptos.

## Testing
```sh
cd sui && sui move test          # 86 tests
cd sui/token_template && sui move build
```
Signature-vector tests use real secp256k1 signatures generated offline
(key `0x4c0883…a033`, the well-known test key); the fin_transfer happy
path doubles as a byte-exactness proof of the payload encoder against an
independent Python implementation.

## File References
- Main contract: [sources/omni_bridge.move](sources/omni_bridge.move)
- Payload types and Borsh: [sources/bridge_types.move](sources/bridge_types.move)
- Borsh primitives: [sources/borsh.move](sources/borsh.move)
- Signature verification / identity helpers: [sources/utils.move](sources/utils.move)
- Token template: [token_template/sources/template_coin.move](token_template/sources/template_coin.move)
