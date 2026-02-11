# OmniBridge Starknet Contract

## Overview
This is a cross-chain bridge contract for Starknet that enables token transfers between Starknet and other chains (primarily NEAR). It uses Ethereum signature verification to validate cross-chain messages.

## Key Components

### Main Contract: `omni_bridge.cairo`
Located at: [starknet/src/omni_bridge.cairo](src/omni_bridge.cairo)

### Core Functions

1. **init_transfer** - Initiates a transfer FROM Starknet TO another chain
   - Burns bridge tokens or locks native tokens
   - Supports optional native token fees
   - Emits `InitTransfer` event for relayers to pick up

2. **fin_transfer** - Finalizes a transfer FROM another chain TO Starknet
   - Requires Ethereum signature verification from the omni bridge
   - Mints bridge tokens or unlocks native tokens
   - Uses nonce tracking to prevent replay attacks

3. **deploy_token** - Deploys a new bridged token on Starknet
   - Requires signature verification
   - Creates a new token contract from `bridge_token_class_hash`
   - Normalizes decimals (max 18)
   - Maintains token mapping between Starknet and NEAR
   - Uses deterministic deployment with salt derived from token ID hash (low 128 bits)

4. **log_metadata** - Logs token metadata for existing tokens
   - Supports both old (felt252) and new (ByteArray) ERC20 standards
   - Used to expose token info to other chains

5. **get_token_address** - View function to query deployed token addresses
   - Takes a NEAR token ID and returns the corresponding Starknet address
   - Returns zero address if token hasn't been deployed yet

### Access Control

- Uses OpenZeppelin's AccessControl component
- **DEFAULT_ADMIN_ROLE**: Can upgrade contract, upgrade bridge tokens, and set pause flags
- **PAUSER_ROLE**: Can pause all operations via `pause_all()`

### Pause Mechanism

Pause flags (bitwise):
- `PAUSE_INIT_TRANSFER` (0x01): Pauses outgoing transfers
- `PAUSE_FIN_TRANSFER` (0x02): Pauses incoming transfers
- `PAUSE_DEPLOY_TOKEN` (0x04): Pauses token deployment
- `PAUSE_ALL` (0xFF): Pauses everything

### Security Features

- Ethereum signature verification using `verify_eth_signature`
- Nonce-based replay protection (bitmap storage for efficiency)
- Borsh serialization for cross-chain message encoding
- Role-based access control for admin operations

## Bridge Architecture

- **NEAR-Centric Design**: All cross-chain transfers route through NEAR Protocol
- Starknet → Other Chain: `init_transfer` on Starknet → NEAR → destination chain
- Other Chain → Starknet: origin chain → NEAR → `fin_transfer` on Starknet
- **Fee Handling**: Fees are deducted on NEAR side before signing transfer messages
  - The `amount` in `fin_transfer` payload is NET of fees (already deducted)
  - Fee recipients are handled by NEAR contract via `sign_transfer`

## Important Notes

- The contract is upgradeable via OpenZeppelin's Upgradeable component
- Token decimals are normalized to max 18 decimals (silently clamped if higher)
- Supports both bridge-deployed tokens (mint/burn) and native tokens (lock/unlock)
- **Trusted Deployer**: Constructor parameters (native_token_address, class_hash) assume trusted deployment

## Testing & Deployment

- When testing, ensure signatures are properly formatted (v, r, s values)
- The `omni_bridge_derived_address` is a derived Ethereum address from the NEAR bridge account that signs cross-chain messages
- Bridge token class hash must be set during deployment for `deploy_token` to work

---

## Security Audit Findings

### 🚨 CRITICAL SEVERITY

#### 1. Token Deployment Collision Risk (Line 221-234)
**Status**: ✅ RESOLVED

**Issue**: Salt was hardcoded to 0, which meant all token deployments would attempt to use the same contract address. This would cause the second (and any subsequent) token deployment to fail, effectively limiting the bridge to only one token. Tokens with the same metadata couldn't be deployed.

**Fix Applied**: Now uses token_id_hash as salt for deterministic deployment (line 234-238):
```cairo
// Verify token hasn't been deployed yet
let token_id_hash = compute_keccak_byte_array(@payload.token);
let existing_token = self.near_to_starknet_token.read(token_id_hash);
assert(existing_token.is_zero(), 'ERR_TOKEN_ALREADY_DEPLOYED');

// Use token_id_hash as salt for deterministic deployment
// Use the low part of the u256 hash to ensure it fits in felt252
let salt: felt252 = token_id_hash.low.into();

let (contract_address, _) = deploy_syscall(
    self.bridge_token_class_hash.read(),
    salt,  // ✓ Deterministic salt based on token ID hash (low 128 bits)
    constructor_calldata.span(),
    false,
)
```

---

### ⚠️ HIGH SEVERITY

#### 2. Unchecked Token Transfer Success (Line 298-304, 340-353)
**Status**: ✅ RESOLVED

**Issue**: External token calls don't explicitly check return values.

**Fix Applied**: Added explicit success checks for all ERC20 transfers:
- `fin_transfer`: Added `assert(success, 'ERR_TRANSFER_FAILED')` after token transfer (line 302-303)
- `init_transfer`: Added `assert(success, 'ERR_TRANSFER_FROM_FAILED')` after token lock (line 345-346)
- `init_transfer`: Added `assert(success, 'ERR_FEE_TRANSFER_FAILED')` after native fee transfer (line 352-353)

**Note**: Bridge token mint/burn operations will still panic on failure as they're controlled by the bridge contract itself.

---

### 📋 MEDIUM SEVERITY

#### 3. Nonce Overflow Protection (Line 336)
**Status**: ✅ SECURE BY DESIGN

**Code**:
```cairo
self.current_origin_nonce.write(self.current_origin_nonce.read() + 1);
```

**Clarification**: Cairo has **built-in overflow protection** for all integer operations. If the nonce reaches `u64::MAX` (2^64 - 1), the addition will automatically panic and revert the transaction.

**Security Property**:
- No explicit check needed - language-level guarantee
- Transaction will revert if overflow attempted
- More efficient than manual checking
- No wraparound possible

**Note**: While reaching 2^64 transfers (~18.4 quintillion) is practically impossible, Cairo's built-in overflow protection ensures safety without additional code.

---


## Design Decisions (NOT Issues)

### ✅ Decimal Normalization with Transparency
**Clarification**: Tokens with decimals > 18 are normalized to 18 decimals (line 225, 444-450), and this is **properly documented** in the `DeployToken` event.

**Implementation**:
```cairo
fn _normalizeDecimals(decimals: u8) -> u8 {
    let maxAllowedDecimals: u8 = 18;
    if (decimals > maxAllowedDecimals) {
        return maxAllowedDecimals;
    }
    return decimals;
}
```

**Transparency via Event**:
The `DeployToken` event includes BOTH values:
```cairo
pub struct DeployToken {
    pub token_address: ContractAddress,
    pub decimals: u8,           // ✓ Normalized value (used on Starknet)
    pub origin_decimals: u8,    // ✓ Original value (from source chain)
    // ... other fields
}
```

**Why This is Safe**:
- Off-chain systems can see both values and understand the normalization
- UIs can warn users if `decimals != origin_decimals`
- The original precision is preserved on the source chain
- Bridge operators can track which tokens required normalization
- Standard practice: Starknet typically uses 18 decimals (like Ethereum)

**Example**: A token with 24 decimals on the source chain will:
1. Be deployed with 18 decimals on Starknet
2. Emit event with `decimals=18` and `origin_decimals=24`
3. Off-chain systems display warning about precision difference
4. Users understand amounts may appear different across chains

### ✅ Public `log_metadata` Function
**Clarification**: The `log_metadata` function (line 145-201) is intentionally public and callable by anyone.

**Design Rationale**:
- Allows anyone to log metadata for tokens they want to bridge
- No state changes - only emits events with token information
- Enables permissionless metadata discovery for off-chain indexers
- Users can verify token details before bridging

**Not a Security Risk**:
- Read-only operation - fetches name, symbol, decimals from token contract
- No privileged operations or state modifications
- Event spam is mitigated by gas costs
- Off-chain indexers can filter/validate events based on token addresses

**Use Case**: Anyone preparing to bridge a token can call `log_metadata` to ensure the token's metadata is indexed for the bridge UI and other off-chain systems.

### ✅ Cross-Chain Replay Protection via Signature Binding
**Clarification**: The contract does NOT have a `destination_chain` field in the `TransferMessagePayload` struct, and this is correct by design.

**How it works**:
1. The destination chain_id is read from storage (line 266): `let chain_id = self.omni_bridge_chain_id.read();`
2. This chain_id is encoded DIRECTLY into the borsh message hash (line 273): `borsh_bytes.append_byte(chain_id);`
3. The message hash is then used for signature verification (line 296)

**Security Property**: If someone tries to replay a signature on a Starknet instance with a different chain_id:
- The reconstructed message hash will be different (because different chain_id is encoded)
- The signature verification will fail
- Replay attack is prevented cryptographically, not through explicit validation

This is more elegant than having a separate `payload.destination_chain` field because:
- Saves calldata space
- The signature is cryptographically bound to the specific chain
- No need for additional validation logic

### ✅ Fee Handling in `fin_transfer`
**Clarification**: Fees are deducted on NEAR side before the transfer message is signed. The `amount` in the payload is already NET of fees. Fee distribution is handled by NEAR's `sign_transfer` function. This is correct by design.

### ✅ Native Token Address Validation
**Clarification**: The contract assumes a trusted deployer who sets correct constructor parameters. This is a reasonable trust assumption for initial deployment.

### ✅ Reentrancy Protection in `fin_transfer`
**Clarification**: The nonce is marked as used (line 264) BEFORE external token calls (lines 298-304), following correct CEI pattern:
1. Check nonce not used (line 261-262)
2. **Set nonce as used (line 264)** ✓
3. Verify signature (line 296)
4. External calls (line 298-304)

Even if the token contract reenters, the nonce check will fail. This is secure.
