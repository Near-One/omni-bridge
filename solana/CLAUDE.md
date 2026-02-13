# Omni Bridge - Solana Program

## Build / Test

```sh
cd solana
anchor build          # build the program
anchor test           # run tests (requires local validator)
```

## Key Architecture

- **`bridge_token_factory`**: Single Anchor program implementing a factory pattern for cross-chain token bridging between Solana and NEAR, supports both SPL Token and Token-2022

### PDA accounts

| Seed | Purpose |
|------|---------|
| `b"config"` | Bridge configuration: admins, derived NEAR bridge address, pause state, bumps |
| `b"authority"` | Program authority PDA — mint authority for wrapped tokens, signs CPIs |
| `b"vault" + mint` | Per-token vault holding locked native tokens |
| `b"sol_vault"` | Holds native SOL for cross-chain transfers + rent reserve for nonce accounts |
| `b"used_nonces" + bucket_id` | Bit-array (1024 nonces/account) preventing replay attacks |
| `b"wrapped_mint" + hashed_token_id` | Mint accounts for bridged tokens from other chains |

### Bridge flow

**Solana → NEAR (initTransfer / initTransferSol)**: User calls `init_transfer`. Native tokens are locked in the vault; bridged tokens are burned. The transfer payload is Borsh-serialized and posted via Wormhole CPI. The NEAR side reads the Wormhole VAA to complete the transfer.

**NEAR → Solana (finalizeTransfer / finalizeTransferSol)**: A relayer calls `finalize_transfer` with a `SignedPayload` containing a NEAR MPC ECDSA signature. The program verifies the signature against `derived_near_bridge_address` stored in config, marks the `destination_nonce` as used, then unlocks (native) or mints (bridged) tokens to the recipient's ATA (auto-created if needed). A confirmation message is posted back via Wormhole.

**Token registration**: Native Solana tokens are registered via `log_metadata` (creates vault, posts metadata to NEAR). Bridged tokens from other chains are deployed via `deploy_token` (requires signed payload, creates mint + Metaplex metadata).

## Security

### Invariants

- **No replay attacks**: Every `destination_nonce` is checked and marked used in a bit-array (`UsedNonces`) before any token operation. A nonce must never be reusable. Nonces are bucketed (1024 per account) and accounts are created on demand
- **No mint/unlock without proof**: Tokens must never be minted or unlocked unless the instruction provides a valid, previously-unused ECDSA signature from the NEAR bridge. "Valid" = recovers to `derived_near_bridge_address`; "previously-unused" = the nonce is unmarked in `UsedNonces`. Any code path that moves tokens outward without both checks is a critical vulnerability
- **Atomic Wormhole posting**: A Wormhole message and its corresponding token lock/burn must occur in the same transaction. Never post a message without the token state change, and never change token state without posting the message — either direction is a bridge invariant violation
- **Malleable signature protection**: The program rejects signatures where `s` is high (`signature.s.is_high()`) to prevent signature malleability
- **Fee < amount**: `fee >= amount` must always revert (`InvalidFee`)
- **Bridged token authority check**: When minting/burning bridged tokens, the program verifies `mint_authority` matches the authority PDA (`InvalidBridgedToken`)
- **Pause granularity**: `init_transfer` and `finalize_transfer` can be paused independently via bit flags (`INIT_TRANSFER_PAUSED = 1`, `FINALIZE_TRANSFER_PAUSED = 2`)

### When modifying the program

- Verify Borsh serialization matches the NEAR side if changing payload structures (see `state/message/` modules and their `serialize_for_near` implementations)
- Consider whether changes affect the pause surface (`INIT_TRANSFER_PAUSED` / `FINALIZE_TRANSFER_PAUSED`)
- The `Config` account has a `padding: [u8; 35]` field — use this for new fields to avoid reallocation

### Security reference

See [SECURITY.md](SECURITY.md) for documented design decisions and known low-severity issues. Consult this before reporting or re-investigating previously reviewed items.
