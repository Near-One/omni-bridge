# NEAR Omni Bridge

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](https://github.com/Near-One/omni-bridge/actions)
[![Release](https://img.shields.io/github/v/release/Near-One/omni-bridge)](https://github.com/Near-One/omni-bridge/releases)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Near-One/omni-bridge/pulls)
[![Dev Support](https://img.shields.io/badge/Dev_Support-2CA5E0?style=flat&logo=telegram&logoColor=white)](https://t.me/chain_abstraction)

The Omni Bridge is a multi-chain asset bridge that facilitates secure and efficient asset transfers between different blockchain networks. It leverages [Chain Signatures](https://docs.near.org/concepts/abstraction/chain-signatures) and it's decentralized [Multi-Party Computation (MPC) service](https://docs.near.org/concepts/abstraction/chain-signatures#multi-party-computation-service) to ensure trustless and decentralized cross-chain asset transfers. 

For more information on how it works, please see [Omni Bridge Documentation](https://docs.near.org/chain-abstraction/omnibridge/overview).

## Supported Networks

- Ethereum (Light client + Chain Signatures)
- Bitcoin (Light client + Chain Signatures)
- Solana (Currently Wormhole, transitioning to Chain Signatures)
- Base (Currently Wormhole, transitioning to Chain Signatures)
- Arbitrum (Currently Wormhole, transitioning to Chain Signatures)

## Contract Addresses

<details>
<summary><strong>Mainnet Addresses</strong></summary>

**Bridge Contracts:**
- Arbitrum: [`0xd025b38762B4A4E36F0Cde483b86CB13ea00D989`](https://arbiscan.io/address/0xd025b38762B4A4E36F0Cde483b86CB13ea00D989)
- Base: [`0xd025b38762B4A4E36F0Cde483b86CB13ea00D989`](https://basescan.org/address/0xd025b38762B4A4E36F0Cde483b86CB13ea00D989)
- NEAR: [`omni.bridge.near`](https://nearblocks.io/address/omni.bridge.near)
- Solana: [`dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe`](https://explorer.solana.com/address/dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe)

**Helper Contracts:**
- NEAR: 
  - [`omni-prover.bridge.near`](https://nearblocks.io/address/omni-prover.bridge.near)
  - [`vaa-prover.bridge.near`](https://nearblocks.io/address/vaa-prover.bridge.near)
</details>

<details>
<summary><strong>Testnet Addresses</strong></summary>

**Bridge Contracts:**
- Arbitrum: [`0x0C981337fFe39a555d3A40dbb32f21aD0eF33FFA`](https://sepolia.arbiscan.io/address/0x0C981337fFe39a555d3A40dbb32f21aD0eF33FFA)
- Base: [`0xa56b860017152cD296ad723E8409Abd6e5D86d4d`](https://sepolia.basescan.org/address/0xa56b860017152cD296ad723E8409Abd6e5D86d4d)
- Ethereum: [`0x68a86e0Ea5B1d39F385c1326e4d493526dFe4401`](https://sepolia.etherscan.io/address/0x68a86e0Ea5B1d39F385c1326e4d493526dFe4401)
- NEAR: [`omni.n-bridge.testnet`](https://testnet.nearblocks.io/address/omni.n-bridge.testnet)
- Solana: [`Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p`](https://explorer.solana.com/address/Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p?cluster=devnet)

**Helper Contracts:**
- NEAR:
  - [`omni-prover.n-bridge.testnet`](https://testnet.nearblocks.io/address/vaa-prover.n-bridge.testnet)
  - [`eth-prover.n-bridge.testnet`](https://testnet.nearblocks.io/address/eth-prover.n-bridge.testnet)
  - [`vaa-prover.n-bridge.testnet`](https://testnet.nearblocks.io/address/vaa-prover.n-bridge.testnet)
</details>

<details>
<summary><strong>Development Testnet Addresses</strong></summary>

**Bridge Contracts:**
- Arbitrum: [`0xd025b38762B4A4E36F0Cde483b86CB13ea00D989`](https://sepolia.arbiscan.io/address/0xd025b38762B4A4E36F0Cde483b86CB13ea00D989)
- Base: [`0x0C981337fFe39a555d3A40dbb32f21aD0eF33FFA`](https://sepolia.basescan.org/address/0x0C981337fFe39a555d3A40dbb32f21aD0eF33FFA)
- Ethereum: [`0x3701B9859Dbb9a4333A3dd933ab18e9011ddf2C8`](https://sepolia.etherscan.io/address/0x3701B9859Dbb9a4333A3dd933ab18e9011ddf2C8)
- NEAR: [`omni-locker.testnet`](https://testnet.nearblocks.io/address/omni-locker.testnet)
- Solana: [`Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p`](https://explorer.solana.com/address/Gy1XPwYZURfBzHiGAxnw3SYC33SfqsEpGSS5zeBge28p?cluster=devnet)

**Helper Contracts:**
- NEAR:
  - [`omni-prover.testnet`](https://testnet.nearblocks.io/address/omni-prover.testnet)
  - [`wormhole-prover-test.testnet`](https://testnet.nearblocks.io/address/wormhole-prover-test.testnet)
</details>

## Transfer Times & Finality

<details>
<summary><strong>Transfer Times Overview</strong></summary>

### NEAR → Other Chains
- Average processing time: ~30 seconds (MPC signatures)

### Other Chains → NEAR
Current finality times:
- Solana: 14s
- Arbitrum: 1066s
- Base: 1026s
- Ethereum: 960s

Additional processing delays:
- OmniBridge transfers relayer: 2s
- Wormhole off-chain validators: 60s
- Ethereum blocks relayer: 60s
</details>

## Token Operations

<details>
<summary><strong>Logging Token Metadata</strong></summary>

### EVM API
```solidity
function logMetadata(address tokenAddress) external
```

### NEAR API
```rust
pub fn log_metadata(&self, token_id: &AccountId) -> Promise 
```

### Solana API
```rust
pub fn log_metadata(ctx: Context<LogMetadata>) -> Result<()>
```

### Using CLI
```bash
cargo run mainnet omni-connector log-metadata --token base:0x<TOKEN_ADDRESS> --base-private-key <KEY>
```

### Using SDK-JS
```typescript
import { getClient } from "omni-bridge-sdk";

// Initialize client for source chain
const client = getClient(ChainKind.Near, wallet);

// Example: Deploy NEAR token to Ethereum
const txHash = await client.logMetadata("near:token.near");
console.log(`Metadata logged with tx: ${txHash}`);
```
</details>

<details>
<summary><strong>Deploying Tokens</strong></summary>

### EVM API
```solidity
function deployToken(bytes calldata signatureData, BridgeTypes.MetadataPayload calldata metadata) payable external returns (address)
```

### NEAR API
```rust
pub fn deploy_token(&mut self, #[serializer(borsh)] args: DeployTokenArgs) -> Promise
```

### Solana API
```rust
pub fn deploy_token(ctx: Context<DeployToken>, data: SignedPayload<DeployTokenPayload>)
```

### Using CLI
```bash
cargo run mainnet omni-connector deploy-token --chain <ChainKind> --source-chain <ChainKind> --tx-hash <LogMetadataTxHash> --base-private-key <KEY>
```
</details>

<details>
<summary><strong>Binding Tokens (NEAR-specific)</strong></summary>

Only needed for NEAR tokens that have been deployed on other chains. This action is typically applied automatically by the relayer.

### NEAR API
```rust
pub fn bind_token(&mut self, #[serializer(borsh)] args: BindTokenArgs) -> Promise
```
</details>

## Transfer Operations

<details>
<summary><strong>Initiating Transfers</strong></summary>

Transfers require a fee, which can be paid in either:
1. The transferred token
2. Native chain token (e.g., ETH, SOL)

**Note:** On NEAR, it isn't possible to attach a deposit in the `ft_transfer_call`, so the native fee should be attached by a separate call to the `storage deposit`

### EVM API
```solidity
// 1. Approve tokens
function approve(address spender, uint256 amount)

// 2. Initiate transfer
function initTransfer(
    address tokenAddress,
    uint128 amount,
    uint128 fee,
    uint128 nativeFee,
    string calldata recipient,
    string calldata message
) payable external
```

### NEAR API
```rust
// 1. Storage deposit
pub fn storage_deposit(&mut self, account_id: Option<AccountId>) -> StorageBalance

// Helper functions
pub fn required_balance_for_account(&self) -> NearToken
pub fn required_balance_for_init_transfer(&self) -> NearToken

// 2. Transfer
fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> PromiseOrValue<U128>
```

### Solana API
```rust
// For SPL tokens
pub fn init_transfer(ctx: Context<InitTransfer>, payload: InitTransferPayload) -> Result<()>

// For native SOL
pub fn init_transfer_sol(ctx: Context<InitTransferSol>, payload: InitTransferPayload) -> Result<()>
```
</details>

## Fee Provider & Status API

<details>
<summary><strong>API Details</strong></summary>

**API Endpoints:**
- Mainnet: `https://mainnet.api.bridge.nearone.org/api/v1`
- Testnet: `https://testnet.api.bridge.nearone.org/api/v1`
- [OpenAPI Specification](https://near-one.github.io/bridge-indexer-rs)

Note: Custom relayers can process transfers with zero fees.
</details>

## SDKs & Tools

- [Bridge SDK (Rust)](https://github.com/Near-One/bridge-sdk-rs)
- [Bridge SDK (JavaScript)](https://github.com/Near-One/bridge-sdk-js)

## Get Involved

We welcome contributions from the community! The code is open source and there are many ways to make meaningful contributions.

### Key Areas for Contribution
- **Chain Integrations**: Help expand support for new blockchain networks
- **Performance Optimization**: Improve transaction speeds and efficiency
- **Security Analysis**: Strengthen the security infrastructure
- **Developer Tools**: Build better tooling and documentation

Bridge infrastructure is a fundamental component of a multi-chain future. Through Chain Signatures, we're creating a more efficient, secure, and scalable approach to cross-chain communication.

Join us in building the future of cross-chain interoperability!
