# BTC Light Client Contract

## Project Overview

A Bitcoin/Litecoin/Dogecoin/Zcash SPV light client implemented as a NEAR Protocol smart contract. It verifies and stores block headers on-chain, enabling trustless verification of Bitcoin (and other UTXO chain) transactions without running a full node. Relayers can submit blocks and they are verified on-chain via proof-of-work validation, difficulty adjustment checks, and chain selection rules.

## Build & Test

Requires: Rust 1.86.0, `cargo-near`, `wasm32-unknown-unknown` target.

```bash
# Build specific chain variant (reproducible via cargo-near)
make build-bitcoin       # default, feature: bitcoin
make build-litecoin      # feature: litecoin
make build-dogecoin      # feature: dogecoin
make build-zcash         # feature: zcash

# Build all chain variants locally (non-reproducible)
make build-local         # all variants
make build-local-bitcoin # single variant

# Run all tests (all features)
make test

# Lint (strict: -D warnings -D clippy::pedantic)
make clippy

# Format check
make fmt
```

## Architecture & Key Concepts

### On-Chain State (`BtcLightClient` in `contract/src/lib.rs`)

- **headers_pool**: `LookupMap<H256, ExtendedHeader>` — all stored headers (main chain + forks)
- **mainchain_height_to_header** / **mainchain_header_to_height**: bidirectional main chain index
- **mainchain_tip_blockhash**: current chain tip
- **gc_threshold**: max number of mainchain blocks to keep in storage. When the mainchain grows beyond this, the oldest mainchain blocks are pruned. GC runs automatically during `submit_blocks()` (with `batch_size` = number of submitted headers) and can also be triggered manually via `run_mainchain_gc(batch_size)`. Only mainchain blocks are deleted; fork/sidechain blocks are not affected

### Block Submission Flow

1. Call `submit_blocks(headers)` with one or more block headers
2. For each header, the contract:
   - Looks up the previous block in storage (rejects unattached blocks)
   - **Verifies PoW**: block hash ≤ target derived from `bits` field
   - **Checks difficulty adjustment**: validates `bits` matches the chain's retarget algorithm
   - Computes accumulated `chain_work`
3. If the block extends the main chain tip → appended directly
4. If it's a fork → stored separately; if fork's `chain_work` > main chain → **automatic reorg**

### Chain Reorganization

When a fork accumulates more work than the main chain:
1. Walk both chains back to common ancestor
2. Demote old main chain blocks (remove from height index)
3. Promote fork blocks to main chain
4. Update tip pointer

**Caveat**: If mainchain blocks near the fork point have been garbage collected, reorg will fail — the contract panics with `PrevBlockNotFound` when it cannot walk the chain back to the common ancestor. This means GC depth must be set conservatively relative to expected fork lengths

### Transaction Inclusion Verification

`verify_transaction_inclusion(ProofArgs)` — SPV proof: given a tx hash, block hash, and merkle proof, verifies the transaction is in the block by recomputing the merkle root.

**Important**: This function is vulnerable to the standard Bitcoin merkle tree second-preimage attack — it may return `true` for an internal node hash rather than a real transaction hash. Block headers do not contain the transaction count, so proof depth cannot be validated on-chain. Callers MUST validate that the `tx_id` corresponds to a valid transaction (e.g., by verifying raw transaction data) before trusting the inclusion proof.


### Garbage Collection

`run_mainchain_gc(batch_size)` removes the oldest mainchain blocks from storage when the mainchain exceeds `gc_threshold`. Only mainchain blocks are pruned; fork/sidechain blocks remain. The `batch_size` parameter limits how many blocks are removed per call to bound gas usage.

## Important Build Flags

The release profile sets `overflow-checks = true` in `Cargo.toml`. This ensures all arithmetic overflows/underflows panic safely instead of wrapping. Do not remove this flag — several view functions rely on it for safe behavior with untrusted inputs.

## Chain-Specific Implementation Details

Each chain lives in its own module and is selected at compile time via mutually exclusive feature flags (`--no-default-features --features "<flag>"`):

| Chain | Module | PoW Hash | Difficulty Algorithm | Special |
|-------|--------|----------|---------------------|---------|
| Bitcoin | `bitcoin.rs` | SHA-256d | Every 2016 blocks, 14-day target | — |
| Litecoin | `litecoin.rs` | Scrypt | Every 2016 blocks, 3.5-day target | Right-shift before multiply |
| Dogecoin | `dogecoin.rs` | Scrypt | Per-block Digishield (after 145k) | AuxPoW merged mining |
| Zcash | `zcash.rs` | Equihash (200,9) | 17-block averaging window, MTP | Blossom hardfork handling |

