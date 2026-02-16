## Overview

Omni-Relayer is an off-chain component of Omni Bridge that relays transfers between NEAR and other networks (Ethereum, Solana, etc.; see `ChainKind` enum in omni-types for full list). It uses the OmniConnector SDK (from bridge-sdk-rs) to orchestrate transfers and Redis as its event queue and state store.

## Build & Development Commands

```bash
cargo build                          # Debug build
cargo build --profile dist           # Release build with thin LTO
cargo test                           # Run all tests
cargo test <test_name>               # Run a single test
cargo test -- --nocapture            # Tests with tracing output
cargo fmt                            # Format code
cargo clippy -- -D warnings          # Lint (matches CI expectations)
cargo run -- --config config.toml    # Run with config file
cargo run -- --help                  # Show CLI flags
```

## Architecture

The binary (`src/main.rs`) spawns a set of concurrent tokio tasks:

1. **Indexers** (`src/startup/`) — watch each chain for bridge events and push them into Redis queues
2. **Event processor** (`src/workers/`) — pulls events from Redis, validates fees, builds proofs, and calls OmniConnector to finalize transfers
3. **Fee bumping** (`src/startup/evm_fee_bumping.rs`) — monitors pending EVM transactions and resubmits with higher gas when needed

### Module Layout

- **`src/config.rs`** — All config structs, deserialized from TOML with env-var substitution (e.g., `INFURA_API_KEY`, `MONGODB_*`, `NEAR_OMNI_*`)
- **`src/startup/`** — Chain indexer initialization and OmniConnector builder
  - `mod.rs` — `build_omni_connector()` assembles all chain bridge clients
  - `near.rs` — NEAR Lake Framework indexer, signer loading
  - `evm.rs` — EVM log subscription (InitTransfer/FinTransfer/DeployToken events), batch processing
  - `solana.rs` — Solana signature polling + Pubsub subscription
  - `bridge_indexer.rs` — MongoDB change stream watcher (alternative to individual chain indexers)
  - `evm_fee_bumping.rs` — Pending tx monitoring and gas price replacement
- **`src/workers/`** — Event processing per chain
  - `mod.rs` — Main `process_events()` loop, `Transfer` enum (Near/Evm/Solana/Utxo/NearToUtxo/UtxoToNear/Fast/FastNearToNear), retry logic with exponential backoff
  - `near.rs`, `evm.rs`, `solana.rs`, `utxo.rs` — Chain-specific transfer finalization
- **`src/utils/`** — Shared helpers
  - `nonce.rs` — `NonceManager` for NEAR and EVM transaction ordering; `EvmNonceManagers` wraps per-chain managers
  - `redis.rs` — Event queue operations (`EVENTS`, `SOLANA_EVENTS`, `STUCK_EVENTS`, `FEE_MAPPING` keys), checkpoint storage
  - `bridge_api.rs` — Fee validation against bridge indexer API
  - `evm.rs` — EVM event types (Solidity definitions via `alloy::sol!`), proof construction
  - `near.rs` — NEAR block finality queries, streamer message handling
  - `solana.rs` — Instruction decoding and Borsh deserialization
  - `storage.rs` — NEAR storage deposit calculations
  - `pending_transactions.rs` — Pending tx tracking struct

### Data Flow

Indexers → Redis event queues → `process_events()` loop → chain-specific worker → OmniConnector SDK → destination chain

### Key External Crates

All bridge SDK crates come from `github.com/Near-One/bridge-sdk-rs` (pinned to a single rev). `omni-types` comes from `github.com/near-one/omni-bridge`. Key types: `ChainKind`, `OmniAddress`, `TransferId`, `TransferMessage`, `Fee`, `OmniBridgeEvent`.

## Coding Conventions

- Rust 2024 edition, toolchain 1.88.0
- Max function/module length: 250 lines (`clippy.toml`)
- Use `tracing` macros for logging, `anyhow::Result` for errors
- Config structs go in `src/config.rs`
- Conventional Commits: `feat:`, `fix:`, `chore:` prefixes
- Test naming: `subject_action_expected` pattern

## Configuration

Copy an `example-*-config.toml` to `config.toml` and fill in secrets. Environment variables are substituted at parse time (see custom deserializers in `config.rs`). Key env vars: `NEAR_OMNI_*`, `NEAR_FAST_*`, `ETH_PRIVATE_KEY`, `SOLANA_PRIVATE_KEY`, `INFURA_API_KEY`, `MONGODB_*`, `GRAFANA_*`, `AWS_*` (for NEAR Lake S3).

Two indexing modes: **bridge indexer** (MongoDB change streams, preferred) or **individual chain indexers** (NEAR Lake + EVM log subscriptions + Solana Pubsub). Controlled by `bridge_indexer` config section.
