## Overview

NEAR smart contracts for the Omni Bridge - a multi-chain asset bridge enabling trustless cross-chain token transfers. Uses Chain Signatures (MPC) for outbound transfers and light clients/Wormhole for inbound proof verification. Supports multiple blockchain networks including some EVM-compatible chains (such as Ethereum, Arbitrum, Base, etc.), Solana, and some UTXO chains (such as Bitcoin, Zcash, etc.). See `ChainKind` enum in omni-types for full list.

## Build Commands

```bash
# Build contracts (run from near/ directory)
cargo near build non-reproducible-wasm --manifest-path omni-bridge/Cargo.toml
cargo near build non-reproducible-wasm --manifest-path omni-token/Cargo.toml
cargo near build non-reproducible-wasm --manifest-path token-deployer/Cargo.toml
cargo near build non-reproducible-wasm --manifest-path omni-prover/evm-prover/Cargo.toml
cargo near build non-reproducible-wasm --manifest-path omni-prover/wormhole-omni-prover-proxy/Cargo.toml
cargo near build non-reproducible-wasm --manifest-path omni-prover/mpc-omni-prover/Cargo.toml

# Testing (run from near/ directory)
cargo nextest run -p omni-tests test_native_fee     # Example: run specific test
cargo nextest run -p <crate> <test_name>            # Template: run any test

# Linting (run from project root)
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
│   ├── wormhole-omni-prover-proxy/    # Wormhole VAA verification
│   └── mpc-omni-prover/               # MPC read-RPC signature verification
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
- `ft_on_transfer()` - Entry point for bridging (receives NEP-141 transfer from token contract)
- `fin_transfer()` - Finalize incoming transfer (requires proof, called by relayer)
- `sign_transfer()` - Request MPC signature for transfer (called by relayer)
- `deploy_token()` - Deploy bridged token on NEAR (requires proof, called by relayer)
- `bind_token()` - Register existing token as bridge-compatible (requires proof, called by relayer)
- `claim_fee()` - Claim accumulated fees (requires proof, called by relayer)

**UTXO Support (btc.rs):**
- `submit_transfer_to_utxo_chain_connector()` - Send to Bitcoin/Zcash (called by relayer)
- `rbf_increase_gas_fee()` - Replace-by-fee for stuck BTC transactions (DAO/RbfOperator only)

## omni-types

Shared types library - defines core types used across all contracts.

**Main types (lib.rs):**
- `ChainKind` - Supported chains enum
- `OmniAddress` - Unified address for any chain
- `TransferId`, `TransferMessage` - Transfer identification and data
- `Fee` - Token fee + native fee structure
- `FastTransfer`, `FastTransferId` - Fast transfer types

**Modules:**
- `errors.rs` - Error types
- `evm/` - EVM-specific types (BlockHeader, Receipt, LogEntry)
- `prover_args.rs` - Prover input structs (`EvmVerifyProofArgs`, `WormholeVerifyProofArgs`, `MpcVerifyProofArgs`)
- `prover_result.rs` - Prover verification results
- `btc.rs` - Bitcoin/UTXO types
- `locker_args.rs` - Function argument structs

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

### mpc-omni-prover
Verifies EVM events using the NEAR MPC network's `verify_foreign_transaction` read-RPC API. Unlike the other provers, this one is fully synchronous (no cross-contract calls) — the relayer obtains the MPC signature off-chain and submits it directly to the prover for local verification.

**State:**
- `mpc_public_key` — hex-encoded compressed secp256k1 public key (33 bytes) of the MPC network
- `chain_kind` — the EVM chain this prover instance verifies (must be an EVM chain)

**Flow:**
1. Deserialize `MpcVerifyProofArgs` and `ForeignTxSignPayload` from input
2. Recompute SHA-256 hash of the borsh-serialized payload, verify it matches the provided hash
3. Verify the MPC secp256k1 signature (using `k256` crate's `PrehashVerifier`) over the payload hash
4. Extract the EVM log from the payload's extracted values, convert to RLP
5. Parse the EVM log via `parse_evm_event` and return typed `ProverResult`

**Dependencies:** Uses `contract-interface` crate from the MPC repo (pinned git rev) for MPC types (`ForeignTxSignPayload`, `EvmLog`, etc.). Does NOT use `near-mpc-sdk` to avoid `near-sdk` version conflicts.

## Code Style

- Rust 2021 edition, 4-space indentation
- Clippy pedantic mode (see `LINT_OPTIONS` in Makefile)
- Test naming: `subject_action_expected` pattern
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`)

## Key Dependencies

- `near-sdk`, `near-contract-standards` - Core NEAR SDK (versions defined in workspace Cargo.toml)
- `near-plugins` - Access control (roles) and upgradeable patterns
- `omni-utils` - Shared utilities (external repo)
- `alloy` - EVM types and RLP encoding
- `k256` - secp256k1 ECDSA verification (used by mpc-omni-prover)
- `contract-interface` - MPC network types from `github.com/near/mpc` (used by mpc-omni-prover)
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

**3. Wormhole Emitter Chain (Correct Design)**
- Chain ID is explicitly encoded in the payload by source bridge (`OmniBridgeWormhole.sol:131-133`)
- Using `token_address.get_chain()` is correct - it reads the chain from the signed payload
- VAA's `emitter_chain` is a Wormhole-specific field; our protocol embeds chain in payload

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
