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

## omni-bridge

The main bridge contract handling cross-chain transfers.

**Key Functions:**
- `ft_on_transfer()` - Entry point for bridging (receives NEP-141 transfer from token contract)
- `fin_transfer()` - Finalize incoming transfer (requires proof, called by relayer)
- `sign_transfer()` - Request MPC signature for transfer (called by relayer)
- `deploy_token()` - Deploy bridged token on NEAR (requires proof, called by relayer)
- `bind_token()` - Register existing token as bridge-compatible (requires proof, called by relayer)
- `claim_fee()` - Claim accumulated fees (requires proof, called by relayer)
- `fin_transfer_as_dao()` - Finalize incoming transfer without proof, for when proof infra cannot attest a valid transfer (DAO only)

**UTXO Support (btc.rs):**
- `submit_transfer_to_utxo_chain_connector()` - Send to Bitcoin/Zcash (called by relayer)
- `rbf_increase_gas_fee()` - Replace-by-fee for stuck BTC transactions (DAO/RbfOperator only)

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
Verifies foreign chain events by calling the NEAR MPC network's `verify_foreign_transaction` API on-chain. The prover initiates the MPC verification as a cross-contract call and validates the response in a callback. Each deployed instance is configured for a specific chain and finality level via `MpcFinality`. Supports any chain supported by the MPC network (currently EVM chains and Starknet, extensible to others).

**State:**
- `mpc_contract_id` — AccountId of the MPC signer contract (e.g. `v1.signer`)
- `finality` — `MpcFinality::Evm(EvmFinality)` or `MpcFinality::Starknet(StarknetFinality)`
- `chain_kind` — the chain this prover instance verifies

**Flow:**
1. Deserialize `MpcVerifyProofArgs` containing `sign_payload`, `proof_kind`, `derivation_path`, `domain_id`, `payload_version`
2. Validate the `sign_payload` (chain and finality must match the prover's configuration)
3. Construct `VerifyForeignTransactionRequestArgs` from the payload's request + the provided derivation fields
4. Cross-contract call to `mpc_contract.verify_foreign_transaction(request_args)` with 1 yoctoNEAR deposit
5. In callback: verify `SHA-256(borsh(sign_payload)) == response.payload_hash`
6. For EVM chains: extract the EVM log, convert to RLP, parse via `parse_evm_event`
7. For Starknet: extract the Starknet log, parse via `parse_starknet_proof` (felt-based event decoding)
8. Return typed `ProverResult`

**Dependencies:** Uses `near-mpc-sdk` crate from the MPC repo (pinned git rev) for MPC types (`ForeignTxSignPayload`, `VerifyForeignTransactionRequestArgs`, `VerifyForeignTransactionResponse`, `EvmLog`, `StarknetLog`, etc.).

## Code Style

- Test naming: `subject_action_expected` pattern
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`)

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
