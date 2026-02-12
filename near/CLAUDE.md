## Overview

NEAR smart contracts for the Omni Bridge - a multi-chain asset bridge enabling trustless cross-chain token transfers. Uses Chain Signatures (MPC) for outbound transfers and light clients/Wormhole for inbound proof verification. Supports Ethereum, Arbitrum, Base, BNB, Polygon, Solana, Bitcoin, and Zcash.

## Build Commands

```bash
# Build all NEAR contracts (reproducible WASM)
make rust-build-near

# Build individual contracts
make rust-build-omni-bridge
make rust-build-omni-token
make rust-build-token-deployer
make rust-build-evm-prover
make rust-build-wormhole-omni-prover-proxy

# Testing
make rust-run-tests                          # Run all tests with cargo nextest
cargo nextest run -p <crate> <test_name>     # Run single test
cargo test -p omni-tests <test_name>         # Integration tests

# Linting
make clippy-near                             # Clippy with pedantic mode
make fmt-near                                # Check formatting
```

## Workspace Structure

```
near/
├── omni-bridge/       # Main bridge contract
├── omni-types/        # Shared type definitions
├── omni-token/        # NEP-141 bridged token implementation
├── token-deployer/    # Token deployment factory
├── omni-prover/
│   ├── evm-prover/                    # EVM light client verification
│   └── wormhole-omni-prover-proxy/    # Wormhole VAA verification
├── omni-tests/        # Integration tests (near-workspaces)
└── mock/              # Test mocks
```

## omni-bridge

The main bridge contract handling cross-chain transfers. Key state in `Contract` struct:

**Token Mappings:**
- `token_id_to_address` - NEAR token ID → foreign chain address
- `token_address_to_id` - Foreign address → NEAR token ID
- `deployed_tokens` - Set of tokens deployed by bridge
- `factories` - Bridge factory addresses per chain

**Transfer State:**
- `pending_transfers` - Transfers awaiting finalization
- `finalised_transfers` - Completed transfer IDs
- `fast_transfers` - Two-leg fast transfer status
- `current_origin_nonce` / `destination_nonces` - Transfer sequencing

**Key Functions:**
- `ft_on_transfer()` - Entry point for bridging (receives NEP-141 transfer)
- `init_transfer()` - Create pending transfer, request MPC signature
- `fin_transfer()` - Finalize incoming transfer (requires proof)
- `sign_transfer()` / `sign_transfer_callback()` - MPC signature flow
- `deploy_token()` - Deploy bridged token on NEAR
- `bind_token()` - Register existing token as bridge-compatible
- `claim_fee()` - Claim accumulated fees

**UTXO Support (btc.rs):**
- `submit_transfer_to_utxo_chain_connector()` - Send to Bitcoin/Zcash
- `utxo_fin_transfer()` - Finalize UTXO incoming transfer
- `rbf_increase_gas_fee()` - Replace-by-fee for stuck BTC transactions

**Access Control Roles:** DAO, PauseManager, UnrestrictedDeposit, UnrestrictedFinalise, TokenControllerUpdater, NativeFeeReceiver

## omni-types

Shared types library used across all crates.

**Core Types:**
- `ChainKind` - Enum: Eth, Near, Sol, Arb, Base, Bnb, Btc, Zcash, Pol, HyperEvm
- `OmniAddress` - Unified address for any chain (enum with chain-specific variants)
- `H160` - 20-byte Ethereum address with EIP-55 checksum
- `TransferId` / `Nonce` - Transfer identification
- `Fee` - Token fee + native fee structure

**Transfer Types:**
- `TransferMessage` - Full transfer data (token, amount, recipient, fee, sender, msg)
- `TransferMessagePayload` - Hashed payload for MPC signing
- `InitTransferMsg` - Parameters to initiate transfer
- `FastFinTransferMsg` - Fast finalization message

**Modules:**
- `errors.rs` - `BridgeError`, `StorageError`, `TokenError`, `ProverError`
- `evm/` - EVM types: `BlockHeader`, `Receipt`, `LogEntry`, event parsing
- `prover_result.rs` - `ProverResult` enum (InitTransfer, FinTransfer, DeployToken, LogMetadata)
- `near_events.rs` - `OmniBridgeEvent` for all bridge events
- `btc.rs` - Bitcoin types: `TxOut`, `UTXOChainConfig`
- `locker_args.rs` - Argument structs for bridge functions
- `prover_args.rs` - `EvmVerifyProofArgs`, `WormholeVerifyProofArgs`
- `mpc_types.rs` - MPC signer communication types

## omni-token

NEP-141 fungible token contract for bridged tokens.

**State:**
```rust
pub struct OmniToken {
    pub controller: AccountId,  // Bridge contract - can mint/burn
    pub token: FungibleToken,   // NEP-141 implementation
    pub metadata: LazyOption<FungibleTokenMetadata>,
}
```

**Traits:** FungibleTokenCore, FungibleTokenResolver, StorageManagement, FungibleTokenMetadataProvider

**Custom Traits (omni_ft/):**
- `MintAndBurn` - `mint()` / `burn()` (controller only)
- `MetadataManagment` - `set_metadata()`

## token-deployer

Factory for deploying omni-token instances using NEAR global contracts.

**State:**
```rust
pub struct TokenDeployer {
    pub global_code_hash: CryptoHash,  // Hash of omni-token WASM
}
```

**Key Method:** `deploy_token()` - Creates account, transfers deposit, deploys global contract, initializes token

**Roles:** DAO, PauseManager, UpgradableCodeStager, UpgradableCodeDeployer, Controller, LegacyController

## omni-prover

### evm-prover
Verifies EVM transaction receipts against light client.

**Flow:**
1. Decode RLP block header, receipt, log entry
2. Verify log is in receipt
3. Verify receipt is in block (Merkle trie proof)
4. Query light client for block hash verification
5. Parse event and return `ProverResult`

### wormhole-omni-prover-proxy
Proxy to Wormhole protocol for chains without light clients (Solana, BNB, EVM L2s).

**Flow:**
1. Receive VAA (Verified Action Approval)
2. Call Wormhole prover's `verify_vaa()`
3. Parse VAA payload in callback
4. Return typed `ProverResult`

## omni-tests

Integration tests using `near-workspaces` sandbox.

**Key Files:**
- `environment.rs` - `TestEnvBuilder` for test setup, contract deployment, token minting
- `helpers.rs` - Build artifacts, test addresses, gas constants
- `init_transfer.rs` - Transfer initiation tests
- `fin_transfer.rs` - Transfer finalization tests
- `fast_transfer.rs` - Two-leg fast transfer tests
- `utxo_fin_transfer.rs` - Bitcoin/Zcash tests
- `omni_token.rs` - Token contract tests
- `native_fee_role.rs` - Access control tests

## mock/

- **mock-prover** - Returns test proof results without verification
- **mock-token** - Basic NEP-141 for testing
- **mock-token-receiver** - Tests `ft_transfer_call` callbacks (can panic or return arbitrary values)
- **mock-utxo-connector** - Simulates Bitcoin connector for testing
- **mock-global-contract-deployer** - Tests global contract deployment

## Code Style

- Rust 2021 edition, 4-space indentation
- Clippy pedantic mode (see `LINT_OPTIONS` in Makefile)
- Test naming: `subject_action_expected` pattern
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`)

## Key Dependencies

- `near-sdk` v5.24.0, `near-contract-standards` v5.24.0
- `near-plugins` - Access control (roles) and upgradeable patterns
- `omni-utils` - Shared utilities (external repo)
- `alloy` - EVM types and RLP encoding
- `near-workspaces` - Integration testing
