# Migration Verification Scripts

A collection of scripts to verify different aspects of the bridge migration process.

## Verify Paused Methods

This script checks that necessary contract methods are properly paused before proceeding with migration.

### Features

- Verifies pause status for all Ethereum and NEAR contracts
- Handles transparent proxy contracts correctly
- Provides color-coded, clear output with detailed explanations
- Includes a migration checklist showing what should be paused

### Setup

1. **Clone the repo**:
   ```bash
   git clone https://github.com/near-one/omni-bridge.git
   git checkout migration
   cd migration/verification
   ```

2. **Install dependencies**:
   ```bash
   pnpm install
   ```

3. **Configure environment**:
   Create a `.env` file with contract addresses. A consolidated `.env` has been prepared that works for **ALL** migration scripts.

   ```bash
   # Ethereum network configuration
   NETWORK_ETH=sepolia
   
   # NEAR network configuration  
   NETWORK_NEAR=testnet
   
   # Contract addresses are included from the main migration .env
   # No need to configure separate .env files for each script
   ```

### Running the script

```bash
pnpm start
```

Or run directly with:

```bash
pnpm tsx verify-paused-methods.ts
```

### Output

The script provides a clear status report showing:
- Which contracts are being checked
- If methods are paused (✓) or not paused (✗)
- A checklist for what should be paused for a safe migration

### Next steps

After verifying the pause status, you can proceed with the migration process by following the next steps in the migration checklist.

## Development

- Formatting: `pnpm format`
- Linting: `pnpm lint`