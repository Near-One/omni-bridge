# A script for checking the finalization of ERC-20 transactions from ETH to NEAR.

Checks that the transactions initiated in the Rainbow Bridge from Ethereum over the past half-day have been successfully finalized on NEAR.

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/Near-One/omni-bridge.git
   cd omni-bridge/migration/checkERC20EthToNearTransfersFinalization
   ```

2. Install dependencies:
   ```bash
   pnpm install
   ```

3. Create an `.env` file in the project root with the following variables:
   ```
   INFURA_API_KEY=your_infura_api_key
   NETWORK_ETH=goerli_or_mainnet
   ERC20_LOCKER=erc20_locker_contract_address
   NETWORK_NEAR=testnet_or_mainnet
   BRIDGE_TOKEN_FACTORY_ACCOUNT_ID=bridge_token_factory_account
   NEAR_ACCOUNT_ID=your_near_account.near
   ```

## Usage

Run the script with:
```bash
pnpm tsx index.ts
```

The script will:
1. Scan the Ethereum blockchain for Locked events from the ERC20 Locker contract
2. Check if each transaction has been finalized on the NEAR blockchain
3. Display the finalization status of all detected transactions

## Monitoring Your Transaction

After initiating a transfer from Ethereum to NEAR:

1. Run this script to verify your transaction appears in the list
2. Continue running the script periodically until you see:
   ```
   All transactions are finalized! You can move to the next step!
   ```

## Environment Variables Explained

| Variable | Description |
|----------|-------------|
| `INFURA_API_KEY` | Your Infura API key for Ethereum network access |
| `NETWORK_ETH` | Ethereum network name (e.g., `mainnet`, `goerli`) |
| `ERC20_LOCKER` | Address of the ERC20 Locker contract on Ethereum |
| `NETWORK_NEAR` | NEAR network name (`testnet` or `mainnet`) |
| `BRIDGE_TOKEN_FACTORY_ACCOUNT_ID` | Bridge token factory account on NEAR |
| `NEAR_ACCOUNT_ID` | Your NEAR account for making view calls |