# OmniBridge Aptos Contract

## Overview
Cross-chain bridge for Aptos, enabling token transfers between Aptos and
other chains via NEAR Protocol. Mirrors the Starknet and EVM implementations
in this repo: see [starknet/src/omni_bridge.cairo](../starknet/src/omni_bridge.cairo)
and [evm/src/omni-bridge/contracts/OmniBridge.sol](../evm/src/omni-bridge/contracts/OmniBridge.sol).

## Architecture
- **NEAR-centric**: All transfers route through NEAR (Aptos ↔ NEAR ↔ other chain).
- **Security**: Ethereum-style ECDSA signature verification against a NEAR
  MPC-derived address. Recovery is performed in `utils::verify_eth_signature`
  using `aptos_std::secp256k1`.
- **Token model**: Bridged Fungible Assets (mint/burn) or native FAs (lock/unlock).
  Bridge-deployed tokens use the Aptos Fungible Asset standard; their
  `MintRef`/`BurnRef`/`TransferRef`/`MutateMetadataRef` are stored as a private
  resource on the FA metadata object's address (`bridge_token::BridgeTokenRefs`).
  The bridge module reaches into that resource via `package fun` only.
- **Named object pattern**: `BridgeState` lives on a deterministic
  `Object<BridgeState>` owned by `@omni_bridge` (seed `b"omni_bridge::state"`).
  The object's `ExtendRef` is stored in `BridgeState` and used on demand to
  derive the bridge signer for FA creation and for moving locked tokens out.
  No resource account. The object is permanently pinned via
  `disable_ungated_transfer` in `initialize` so a deployer-key compromise
  cannot move locked funds out.
- **Role-based access control**: `BridgeState.roles: Table<u8, vector<address>>`
  maps each role discriminant to a list of holder addresses. Three roles
  ship today (`ROLE_ADMIN`, `ROLE_PAUSER`, `ROLE_METADATA_ADMIN`); each role
  can have any number of holders, all equally privileged. The `Admin` role
  grants/revokes any role (including itself) via `grant_role`/`revoke_role`.
  Revoking the last `Admin` aborts with `E_CANNOT_REMOVE_LAST_ADMIN`.

## Module Layout

| Module | Purpose |
|--------|---------|
| `omni_bridge::omni_bridge` | Main contract: init, deploy_token, init_transfer, fin_transfer, log_metadata, role management, pause, metadata mutation, events, views |
| `omni_bridge::bridge_token` | Fungible Asset wrapper exposing `create`/`mint`/`burn`/`mutate_metadata` as `package fun` (package-internal only). Holds the per-token capability bundle on the FA object's address |
| `omni_bridge::bridge_types` | Payload structs (`MetadataPayload`, `TransferMessagePayload`) and their Borsh encoders. Events live in `omni_bridge` because Aptos requires `#[event]` and emit-site in the same module |
| `omni_bridge::borsh` | Borsh sequence encoders (`encode_string`, `encode_byte_vec`). Fixed-width integers and addresses delegate to `std::bcs::to_bytes` directly at call sites (BCS == Borsh for those types) |
| `omni_bridge::utils` | `verify_eth_signature` (secp256k1 + keccak256), `normalize_decimals` |

## Borsh Encoding

The payload encoding is byte-identical to the Starknet implementation so the
NEAR MPC signature can be reused without per-chain branching on the NEAR
side. In particular:

- Addresses are 32 bytes big-endian (Aptos addresses are 32 bytes natively).
- `TransferMessagePayload` carries the destination chain id twice — once
  before `token_address` and once before `recipient` — as the OmniAddress
  tag. The destination chain id is mixed into the hash, **not** the payload.
- `message` is *not* wrapped with an `Option` tag: `None` contributes
  nothing; `Some(bytes)` contributes only the length-prefixed bytes.
- `fee_recipient` *is* a standard Borsh `Option<String>`.
- Fixed-width integer encoders use `bcs::to_bytes(&val)` directly at the
  call site since BCS and Borsh are byte-identical for those types.
  Only the 4-byte LE length prefix for sequences needs custom code (BCS
  uses ULEB128 there).

## Important Notes

### Design Decisions
1. **Deterministic FA addresses**: created via `object::create_named_object`
   using the NEAR token id's UTF-8 bytes directly as the seed (no keccak —
   Aptos's `create_named_object` accepts arbitrary-length seeds, unlike
   Starknet's `felt252` salt). Same token id always maps to the same Aptos
   FA address.
2. **Permissionless `deploy_token` / `log_metadata`**: the MPC signature is
   the authorization for deploys, and `log_metadata` is just an event
   emission anyone can trigger.
3. **Decimals normalization**: capped at **8** (silently clamped) — not
   18 like Starknet/EVM. Aptos FA amounts are `u64`; at 18 decimals one
   token already consumes 1e18 base units, leaving only ~18 tokens of
   headroom in `u64::MAX`. 8 decimals matches APT's native precision and
   gives ~1.84e11 tokens of room. The NEAR side already scales between
   source-chain and target-chain decimals, so a tighter cap on this side
   only reduces precision, not value.
4. **u128 → u64 amount**: Aptos FA amounts are `u64`; the bridge payload
   uses `u128` for cross-chain compatibility. Amounts are explicitly
   bounded by `MAX_U64_AS_U128` before the FA call.
5. **Bridge object address**: locked-token custody lives at
   `object::create_object_address(&@omni_bridge, b"omni_bridge::state")`.
   Off-chain integrations can compute it without an on-chain call. The
   `bridge_object_address()` view returns the same.
6. **Role discriminants are `u8`**: Aptos disallows custom enum types as
   entry/view parameters, so the role table key is a `u8`. Numeric values
   are part of the ABI — never reorder. The `all_roles()` view returns the
   `(name, id)` registry for off-chain discovery.
7. **Last-admin guard**: `revoke_role(Admin, last_admin)` aborts with
   `E_CANNOT_REMOVE_LAST_ADMIN`. Prevents bricking via accidental rotation.

### Security Invariants
- **No replay**: `destination_nonce` is checked against `completed_transfers`
  and marked used *before* any token transfer in `fin_transfer`. `origin_nonce`
  is incremented atomically at the top of `init_transfer`.
- **State before external calls**: pause checks, nonce marking, and role
  checks happen before `bridge_token::mint`, `primary_fungible_store::transfer`,
  etc.
- **No token release without signature**: `fin_transfer` performs Ethereum
  signature verification before any mint/transfer.
- **Event completeness**: `InitTransfer` carries every field the NEAR side
  needs to reconstruct the transfer (`sender`, `token_address`,
  `origin_nonce`, `amount`, `fee`, `native_fee`, `recipient`, `message`).
- **Capability confinement**: bridge token mint/burn/metadata refs live in
  a private resource defined inside `bridge_token`. Only that module can
  borrow them, and the mutating helpers are `package fun` — callable only
  from inside the `omni_bridge` package.
- **Bridge object pinned**: `initialize` calls `disable_ungated_transfer`
  on the bridge object, so a deployer-key compromise cannot move the
  object (and its custody balance) out.

### Bitmap layout
`completed_transfers: Table<u64, u128>` packs nonces 128-per-slot
(`slot = nonce / 128`, `bit = nonce % 128`). This matches the Starknet
contract's 251-per-slot bitmap in spirit (Cairo's `felt252` allows 251
bits; Move's native `u128` is the natural fit).

### Roles can grow large but linear-scan
Each gated entry function asserts membership via a linear scan over the
role's holder vector. Expected role list sizes are 1–5 addresses; at that
scale the cost is negligible. If a role ever grows to dozens of holders,
swap to `aptos_std::ordered_map<address, bool>` per role.

## Testing
Run unit tests with:

```sh
aptos move test --named-addresses omni_bridge=0xCAFE
```
