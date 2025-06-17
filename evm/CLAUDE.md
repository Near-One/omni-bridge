# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OmniBridge is a cross-chain bridge facilitating token transfers between NEAR Protocol and Ethereum-compatible chains. The EVM side includes core bridge contracts, an eNear proxy system, and multi-chain support for Ethereum, Arbitrum, and Base networks.

## Development Commands

**Build and Test:**
```bash
bun run build          # Compile contracts
bun run test           # Run all tests
bun run coverage       # Generate test coverage report
bun run clean          # Clean build artifacts
```

**Linting and Formatting:**
```bash
bun run lint:js        # Check TypeScript/JavaScript with Biome
bun run lint:js:fix    # Fix TypeScript/JavaScript issues
bun run lint           # Check Solidity formatting with Prettier
bun run lint:fix       # Fix Solidity formatting
```

**Security Auditing:**
```bash
bun run secaudit       # Run Slither static analysis
```

**Storage Layout:**
```bash
bun run check  # Get contract storage layout
```

**Deployment Tasks:**
```bash
# Deploy eNear proxy
bun run deploy-e-near-proxy --enear "<address>" --admin "<address>" --network sepolia

# Deploy token implementation
bun run deploy-token-impl --network sepolia

# Deploy OmniBridge factory
bun run deploy-bridge-token-factory --bridge-token-impl <address> --near-bridge-account-id <account_id> --network sepolia

# Verify contracts
bun run verify <address> --network sepolia

# Set token metadata
bun run set-metadata-ft --near-token-account <id> --name "<name>" --symbol "<symbol>" --factory <address> --network sepolia
```

## Architecture

**Core Components:**
- `src/omni-bridge/contracts/OmniBridge.sol` - Main bridge factory contract using UUPS proxy pattern
- `src/omni-bridge/contracts/BridgeToken.sol` - ERC20 token template for bridged assets
- `src/eNear/contracts/ENearProxy.sol` - Proxy for existing eNear token control
- `src/common/Borsh.sol` - NEAR protocol data serialization utilities

**Key Patterns:**
- **Upgradeable Contracts**: Uses OpenZeppelin UUPS proxy pattern for upgradeability
- **Access Control**: Role-based permissions with OpenZeppelin AccessControl
- **Cross-chain Messaging**: ECDSA signature verification for NEAR-to-EVM messages
- **Token Factory**: Dynamic ERC20 token creation for new bridged assets

**Network Configuration:**
- Multi-network support via Hardhat config (mainnet, testnets, L2s)
- Custom `omniChainId` field for cross-chain identification
- Optional Wormhole integration via `wormholeAddress` config

## Testing

**Test Structure:**
- `tests/BridgeToken.ts` - Bridge token functionality
- `tests/BridgeTokenWormhole.ts` - Wormhole integration
- `tests/eNearProxy.test.ts` - eNear proxy functionality
- `tests/helpers/signatures.ts` - Signature verification utilities

**Test Environment:**
- Hardhat network with ethers.js and Chai matchers
- Network helpers for blockchain simulation
- TypeChain-generated type bindings

## Development Environment

**Required Environment Variables:**
```bash
INFURA_API_KEY=      # For network connections
EVM_PRIVATE_KEY=     # For contract deployment
ETHERSCAN_API_KEY=   # For contract verification
ARBISCAN_API_KEY=    # For Arbitrum verification
BASESCAN_API_KEY=    # For Base verification
```

**Type Generation:**
TypeScript bindings are auto-generated in `typechain-types/` directory when compiling contracts.

**Code Quality Tools:**
- **Biome**: JavaScript/TypeScript linting and formatting (100 char line width, double quotes)
- **Prettier**: Solidity formatting with solidity plugin
- **Slither**: Security static analysis with custom configuration excluding common false positives

## Key Files

- `hardhat.config.ts` - Network configs, custom tasks, compiler settings
- `biome.json` - JavaScript/TypeScript linting rules
- `slither.config.json` - Security analysis configuration
- `utils/kdf.ts` - Key derivation functions for multi-chain address generation