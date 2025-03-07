/**
 * NEAR-ETH Bridge Transaction Verification Script
 * 
 * This script checks for ERC20 token locking events on Ethereum and verifies 
 * their finalization status on the NEAR blockchain.
 */

import { connect, keyStores } from "near-api-js";
import dotenv from 'dotenv';
import { Header } from 'eth-object';
import { ethers } from "ethers";
import { Formatter } from "@ethersproject/providers";
import process from 'node:process';
import { b } from '@zorsh/zorsh';
import chalk from 'chalk'; // For colorized console output
import { cleanEnv, str } from 'envalid'; // For environment validation

// Load environment variables
dotenv.config();

// Validate environment variables with envalid
const ENV = cleanEnv(process.env, {
  ERC20_LOCKER: str({ desc: 'Ethereum address of the ERC20 Locker contract' }),
  NETWORK_ETH: str({ choices: ['mainnet', 'goerli', 'sepolia'] }),
  INFURA_API_KEY: str(),
  NETWORK_NEAR: str({ choices: ['mainnet', 'testnet'] }),
  BRIDGE_TOKEN_FACTORY_ACCOUNT_ID: str(),
  NEAR_ACCOUNT_ID: str({ example: 'example.near' }),
});

// Contract ABI for the ERC20 Locker
const ABI = [
  "event Locked(address indexed token, address indexed sender, uint256 amount, string accountId)"
];

// Configure Ethereum provider
const provider = new ethers.JsonRpcProvider(
  `https://${ENV.NETWORK_ETH}.infura.io/v3/${ENV.INFURA_API_KEY}`
);
const contract = new ethers.Contract(ENV.ERC20_LOCKER, ABI, provider);

// Define EVM proof schema
const EVMProofSchema = b.struct({
  log_index: b.u64(),
  log_entry_data: b.vec(b.u8()),
  receipt_index: b.u64(),
  receipt_data: b.vec(b.u8()),
  header_data: b.vec(b.u8()),
  proof: b.vec(b.vec(b.u8()))
});

type EVMProof = b.infer<typeof EVMProofSchema>;

/**
 * Initialize NEAR connection
 * @returns NEAR connection object
 */
async function initNear() {
  try {
    const keyStore = new keyStores.InMemoryKeyStore();
    
    return await connect({
      networkId: ENV.NETWORK_NEAR,
      keyStore,
      nodeUrl: `https://rpc.${ENV.NETWORK_NEAR}.near.org`,
      walletUrl: `https://wallet.${ENV.NETWORK_NEAR}.near.org`,
      helperUrl: `https://helper.${ENV.NETWORK_NEAR}.near.org`
    });
  } catch (error) {
    console.error(chalk.red('Error initializing NEAR connection:'), error);
    throw error;
  }
}

/**
 * Convert EVM proof to buffer
 * @param proof - EVM proof object
 * @returns Buffer representation of the proof
 */
function evmProofToBuffer(proof: EVMProof) {
  return Buffer.from(EVMProofSchema.serialize(proof));
}

/**
 * Check if a proof has been used on NEAR
 * @param proof - EVM proof to check
 * @returns Boolean indicating if the proof has been used
 */
async function isUsedProof(proof: EVMProof) {
  try {
    const near = await initNear();
    const account = await near.account(ENV.NEAR_ACCOUNT_ID);

    const result = await account.viewFunction({
      contractId: ENV.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID,
      methodName: "is_used_proof",
      args: proof,
      stringify: evmProofToBuffer
    });

    return result;
  } catch (error) {
    console.error(chalk.red('Error checking if proof is used:'), error);
    throw error;
  }
}

/**
 * Get and process the latest Locked events
 */
async function getLatestEvents() {
  try {
    // Get latest block number and calculate the range for event querying
    const latestBlock = await provider.getBlockNumber();
    const fromBlock = Math.max(0, latestBlock - 3000); // Ensure fromBlock isn't negative
    
    console.log(chalk.blue(`Scanning blocks from ${fromBlock} to ${latestBlock}...`));

    // Query for Locked events
    const events = await contract.queryFilter("Locked", fromBlock, latestBlock);
    console.log(chalk.green(`${events.length} Locked transactions detected`));
    
    let notFinalizedCount = 0;
    
    // Process each event
    for (let i = 0; i < events.length; i++) {
      const event = events[i];
      console.log(chalk.cyan(`\nProcessing transaction ${i+1}/${events.length}: ${event.transactionHash}`));
      
      try {
        // Get block data
        const block = await provider.send(
          'eth_getBlockByNumber',
          [ethers.toBeHex(event.blockNumber), false]
        );
        const headerRlp = Header.fromRpc(block).serialize();

        // Get transaction receipt
        const rpcObjFormatter = new Formatter();
        const receipt = rpcObjFormatter.receipt(
          await provider.send('eth_getTransactionReceipt', [event.transactionHash])
        );

        // Find log index in the receipt
        const logIndexInArray = receipt.logs.findIndex(
          l => l.logIndex === event.index
        );

        if (logIndexInArray === -1) {
          console.warn(chalk.yellow(`Warning: Log not found in receipt for transaction ${event.transactionHash}`));
          continue;
        }

        // Create proof
        const proofLight: EVMProof = {
          "log_index": BigInt(logIndexInArray),
          "log_entry_data": Uint8Array.from([]),
          "receipt_index": BigInt(event.transactionIndex),
          "receipt_data": Uint8Array.from([]),
          "header_data": Uint8Array.from(headerRlp),
          "proof": [],
        };

        // Check if proof is used
        const isFinalized = await isUsedProof(proofLight);

        if (!isFinalized) {
          notFinalizedCount += 1;
          console.log(chalk.yellow(`Transaction ${event.transactionHash} is NOT finalized`));
        } else {
          console.log(chalk.green(`Transaction ${event.transactionHash} is finalized`));
        }
      } catch (error) {
        console.error(chalk.red(`Error processing transaction ${event.transactionHash}:`), error);
      }
    }

    // Print summary
    console.log(chalk.blue('\nSummary:'));
    if (notFinalizedCount > 0) {
      console.log(chalk.yellow(`${notFinalizedCount} transactions are NOT finalized. Please wait for finalization!`));
    } else if (events.length > 0) {
      console.log(chalk.green('All transactions are finalized! You can move to the next step!'));
    } else {
      console.log(chalk.blue('No transactions to process in the specified range.'));
    }
    
  } catch (error) {
    console.error(chalk.red('Error in getLatestEvents:'), error);
  }
}

// Run the script
(async () => {
  try {
    console.log(chalk.blue('Starting NEAR-ETH bridge transaction verification...'));
    console.log(chalk.blue(`Network: ETH ${ENV.NETWORK_ETH} → NEAR ${ENV.NETWORK_NEAR}`));
    
    await getLatestEvents();
  } catch (error) {
    console.error(chalk.red('Fatal error:'), error);
    process.exit(1);
  }
})();