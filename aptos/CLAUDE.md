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
  `MintRef`/`BurnRef`/`TransferRef` are stored as a private resource on the FA
  metadata object's address (`bridge_token::BridgeTokenRefs`).
- **Named object pattern**: `BridgeState` lives on a deterministic
  `Object<BridgeState>` owned by `@omni_bridge` (seed
  `b"omni_bridge::state"`). The object's `ExtendRef` is stored in
  `BridgeState` and used on demand to derive the bridge signer for FA
  creation and for moving locked tokens out. No resource account — this
  aligns with the `write-contracts` skill's "Never use resource accounts
  (use named objects instead)" guidance.
- **Access control**: Two roles — `admin` (full admin) and `pauser`
  (`pause_all` only). Both addresses are stored in `BridgeState`.
- **Modern Move (V2)**: Receiver-style method calls (`v.push_back(x)`,
  `t.contains(k)`, `payload.metadata_to_borsh()`), vector indexing
  (`bytes[i]`), `for (i in 0..n)` range loops, and `package fun` for
  cross-module-restricted functions in `bridge_token`.

## Module Layout

| Module | Purpose |
|--------|---------|
| `omni_bridge::omni_bridge` | Main contract: init, deploy_token, init_transfer, fin_transfer, log_metadata, admin, views |
| `omni_bridge::bridge_token` | Fungible Asset wrapper exposing `create`/`mint`/`burn` as `package fun` (package-internal only) |
| `omni_bridge::bridge_types` | Payload structs, events, and Borsh encoding for cross-chain compatibility |
| `omni_bridge::borsh` | Low-level Borsh encoders (`u32`, `u64`, `u128`, `string`, `address`, `byte_vec`) |
| `omni_bridge::utils` | `verify_eth_signature`, `normalize_decimals` |

## Core Functions

| Function | Purpose | Access |
|----------|---------|--------|
| `initialize` | Set up bridge state and resource account | Deployer (once) |
| `init_transfer` | Send tokens from Aptos to another chain | Public |
| `fin_transfer` | Receive tokens from another chain (requires signature) | Public |
| `deploy_token` | Deploy a new bridged FA (requires signature) | Public |
| `log_metadata` | Emit metadata event for an existing FA | Public |
| `get_token_address` | View deployed token by NEAR token id | View |
| `is_bridge_token_addr` | View whether an address is a bridge token | View |
| `is_transfer_finalised` | View whether a destination nonce has been used | View |
| `set_pause_flags` / `pause_all` | Pause operations | Admin/Pauser |
| `set_admin`, `set_pauser`, `set_near_bridge_derived_address` | Admin config | Admin |

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

## Important Notes

### Design Decisions
1. **Deterministic FA addresses**: Created via `object::create_named_object`
   using `keccak256(near_token_id)` as the seed. Same NEAR token id always
   maps to the same Aptos FA address, identical to the Starknet salt scheme.
2. **Public `deploy_token` / `log_metadata`**: Intentionally permissionless;
   the MPC signature is the authorization for deploys, and `log_metadata`
   is only an event emission.
3. **Decimals normalization**: Capped at 18 (silently clamped). Matches
   Starknet/EVM.
4. **u128 → u64 amount**: Aptos FA amounts are `u64`; the bridge payload
   uses `u128` for cross-chain compatibility. Amounts are explicitly
   bounded by `u64::MAX` before the FA call.
5. **Bridge object address**: Locked-token custody lives at
   `object::create_object_address(&@omni_bridge, b"omni_bridge::state")`.
   Off-chain integrations can compute it without on-chain calls. Expose
   via the `bridge_object_address()` view.

### Security Invariants
- **No replay**: `destination_nonce` is checked against `completed_transfers`
  and marked used *before* any token transfer in `fin_transfer`. `origin_nonce`
  is incremented atomically at the top of `init_transfer`.
- **State before external calls**: Pause checks and nonce marking happen
  before `bridge_token::mint`, `primary_fungible_store::transfer`, etc.
- **No token release without signature**: `fin_transfer` performs Ethereum
  signature verification before any mint/transfer.
- **Event completeness**: `InitTransfer` carries every field the NEAR side
  needs to reconstruct the transfer (`sender`, `token_address`,
  `origin_nonce`, `amount`, `fee`, `native_fee`, `recipient`, `message`).
- **MintRef confinement**: Bridge token mint/burn refs are stored as a
  resource defined inside `bridge_token`. Only that module can borrow them,
  and only the `omni_bridge` module is friend-listed to call its mint/burn.

### Bitmap layout
`completed_transfers: Table<u64, u128>` packs nonces 128-per-slot
(`slot = nonce / 128`, `bit = nonce % 128`). This matches the Starknet
contract's 251-per-slot bitmap in spirit (Cairo's `felt252` happens to
allow 251 bits; Move's native `u128` is the natural fit).

## Testing
Run unit tests with:

```sh
aptos move test --named-addresses omni_bridge=0xCAFE
```

Coverage includes:
- Borsh encoding for all integer widths, strings, byte vectors, and addresses
- Metadata and TransferMessage Borsh layouts (length and field offsets)
- Nonce bitmap edge cases (word boundary at 127/128, idempotent set)
- Pause flag access control
- Bridge token create / mint / burn round-trip
- Decimal normalization cap

## File References
- Main contract: [sources/omni_bridge.move](sources/omni_bridge.move)
- Bridge token: [sources/bridge_token.move](sources/bridge_token.move)
- Type definitions and Borsh: [sources/bridge_types.move](sources/bridge_types.move)
- Borsh primitives: [sources/borsh.move](sources/borsh.move)
- Signature verification: [sources/utils.move](sources/utils.move)
