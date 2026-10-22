# OmniBridge Starknet Contract

## Overview
Cross-chain bridge contract enabling token transfers between Starknet and other chains via NEAR Protocol.

## Architecture
- **NEAR-Centric**: All transfers route through NEAR (Starknet ↔ NEAR ↔ Other Chain)
- **Security**: Ethereum signature verification using derived NEAR account address
- **Token Model**: Bridge-deployed tokens (mint/burn) or native tokens (lock/unlock)
- **Access Control**: OpenZeppelin AccessControl with DEFAULT_ADMIN_ROLE and PAUSER_ROLE

## Key Implementation Details

### Token Deployment
- **Deterministic addresses**: Uses `keccak(token_id).low` as salt
- **Decimal normalization**: Max 18 decimals (silently clamped)
- **Anti-collision**: Checks `near_to_starknet_token` mapping before deployment

### Transfer Security
- **Signature verification**: All incoming transfers require valid NEAR-signed message
- **Nonce protection**: Bitmap storage prevents replay attacks
- **CEI pattern**: Nonce marked used before external calls
- **Transfer validation**: Explicit success checks for ERC20 operations

### Fee Handling
- Fees are deducted on NEAR side before signing
- `fin_transfer` receives net amount (post-fee)
- Optional native token fees in `init_transfer` (e.g., for gas)

## Important Notes

### Design Decisions
1. **Chain ID binding**: Destination chain_id encoded in message hash (not in payload) - prevents cross-chain replay
2. **Public `log_metadata`**: Intentionally permissionless for token discovery
3. **Salt uses low 128 bits**: Full u256 hash doesn't fit in felt252
4. **Trusted deployer**: Constructor params (native_token, class_hash) assume honest deployment

### Security Properties
- ✅ Cairo built-in overflow protection (no manual checks needed)
- ✅ Deterministic token addresses (same token ID → same address)
- ✅ Reentrancy safe (CEI pattern + nonce check)
- ✅ Transfer success validation for all external ERC20 calls

