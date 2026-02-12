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

## Security Audit Notes

### Common False Positives to Avoid

When auditing this codebase, these patterns are NOT vulnerabilities:

**1. Fast Transfer Fee Manipulation (NOT a vulnerability)**
- `FastTransferId` is computed from the entire struct including fee
- If relayer specifies wrong fee, IDs won't match when proof arrives
- Result: Relayer LOSES their fronted tokens, cannot profit
- The design is self-protecting

**2. Decimal Arithmetic Underflow (NOT a vulnerability)**
- Design expects `origin_decimals >= decimals` (normalization to lower precision)
- Workspace has `overflow-checks = true` in Cargo.toml
- Misconfiguration causes panic (correct fail-safe), not silent corruption

**3. Wormhole Emitter Chain Mismatch (NOT exploitable)**
- Uses `token_address.get_chain()` instead of VAA's `emitter_chain`
- Exploitation requires 2^-160 address collision (cryptographically impossible)

**4. Gas Griefing via Storage Actions (NOT a vulnerability)**
- Caller provides their own `storage_deposit_actions`
- Bad inputs only harm the caller themselves (self-griefing)

**5. Signer ID Storage Manipulation (NOT profitable)**
- Attacker must spend their own tokens to create transfer
- Storage is refunded when transfer completes
- No profit mechanism for attacker

**6. Missing Emitter Validation in Prover (Correct Architecture)**
- Prover verifies cryptographic proof validity
- Bridge callback validates emitter against registered factories
- This separation of concerns is intentional and correct

**7. finish_withdraw_v2 Arbitrary Calls (Requires DAO Compromise)**
- Only callable by tokens in `deployed_tokens`
- `omni-token` (what bridge deploys) doesn't call this function
- Exploitation requires DAO to add malicious token (out of scope)

### Security Analysis Checklist

When reviewing changes to this codebase:

1. **Check overflow-checks**: Verify `Cargo.toml` still has `overflow-checks = true`
2. **Trace ID computations**: Changes to structs used in ID hashing affect matching logic
3. **Verify callback validation**: Ensure bridge callbacks validate emitter addresses
4. **Check .detach() usage**: Detached promises should only be used for non-critical operations
5. **Trust boundaries**: DAO, RbfOperator, UTXO Connectors are semi-trusted roles
6. **Storage refunds**: Ensure storage owners receive refunds on transfer completion
