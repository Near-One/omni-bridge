## Overview

Omni-Relayer is an off-chain component of Omni Bridge that relays transfers between NEAR and other networks (Ethereum, Solana, etc.; see `ChainKind` enum in omni-types for full list). It uses the OmniConnector SDK (from bridge-sdk-rs) to orchestrate transfers and Redis as its event queue and state store.

## Build & Development Commands

```bash
cargo run -- --config config.toml    # Run with config file
cargo run -- --help                  # Show CLI flags
cargo build                          # Debug build
cargo fmt                            # Format code
cargo clippy -- -D warnings          # Lint (matches CI expectations)
```

## Architecture

The binary (`src/main.rs`) spawns a set of concurrent tokio tasks:

1. **Indexers** (`src/startup/`) — watch each chain for bridge events and push them into Redis queues
2. **Event processor** (`src/workers/`) — pulls events from Redis, validates fees, builds proofs, and calls OmniConnector to finalize transfers
3. **Fee bumping** (`src/startup/evm_fee_bumping.rs`) — monitors pending EVM transactions and resubmits with higher gas when needed

### Event Flow

1. **Indexers watch chains** (`src/startup/`)
   - NEAR: Lake Framework streams blocks → detects bridge events
   - EVM: Log subscription → catches InitTransfer/FinTransfer/DeployToken
   - Solana: Signature polling + Pubsub
   - Alternative: MongoDB change streams (bridge indexer API)

2. **Events → Redis queues** (`src/utils/redis.rs`)
   - Separate queues: `EVENTS`, `SOLANA_EVENTS`, `STUCK_EVENTS`
   - Checkpoint storage for resuming

3. **Event processor** (`src/workers/mod.rs`)
   - Pulls events from Redis in `process_events()` loop
   - Validates fees against bridge indexer API
   - Builds proofs for destination chain
   - Retry with exponential backoff on failure

4. **Chain-specific finalization** (`src/workers/{near,evm,solana,utxo}.rs`)
   - Calls OmniConnector SDK to submit proof + finalize transfer
   - Uses `NonceManager` for transaction ordering

5. **Fee bumping** (`src/startup/evm_fee_bumping.rs`)
   - Monitors pending EVM transactions
   - Resubmits with higher gas if stuck

**Key modules:**
- `src/config.rs` - Config with env-var substitution
- `src/utils/` - Nonce management, proof construction, storage calculations

### Data Flow

Indexers → Redis event queues → `process_events()` loop → chain-specific worker → OmniConnector SDK → destination chain

### Key External Crates

All bridge SDK crates come from `github.com/Near-One/bridge-sdk-rs` (pinned to a single rev). `omni-types` comes from `github.com/near-one/omni-bridge`. Key types: `ChainKind`, `OmniAddress`, `TransferId`, `TransferMessage`, `Fee`, `OmniBridgeEvent`.

## Coding Conventions

- Max function/module length: 250 lines (`clippy.toml`)
- Use `tracing` macros for logging, `anyhow::Result` for errors
- Config structs go in `src/config.rs`
- Conventional Commits: `feat:`, `fix:`, `chore:` prefixes
- Test naming: `subject_action_expected` pattern

## Configuration

Copy an `example-*-config.toml` to `config.toml` and fill in secrets. Environment variables are substituted at parse time (see custom deserializers in `config.rs`). Key env vars: `NEAR_OMNI_*`, `NEAR_FAST_*`, `ETH_PRIVATE_KEY`, `SOLANA_PRIVATE_KEY`, `INFURA_API_KEY`, `MONGODB_*`, `GRAFANA_*`, `AWS_*` (for NEAR Lake S3).

Two indexing modes: **bridge indexer** (MongoDB change streams, preferred) or **individual chain indexers** (NEAR Lake + EVM log subscriptions + Solana Pubsub). Controlled by `bridge_indexer` config section.
