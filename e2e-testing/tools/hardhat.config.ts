import "@nomicfoundation/hardhat-chai-matchers"
import "@nomicfoundation/hardhat-ethers"
import "@nomicfoundation/hardhat-verify"
import "@openzeppelin/hardhat-upgrades"
import "@typechain/hardhat"
import * as dotenv from "dotenv"
import "hardhat-storage-layout"
import type { HardhatUserConfig } from "hardhat/config"
import "solidity-coverage"
import { task } from "hardhat/config"

import "hardhat/types/config"
import * as fs from "node:fs"

declare module "hardhat/types/config" {
  interface HttpNetworkUserConfig {
    omniChainId: number
    wormholeAddress?: string
  }
}

dotenv.config()

const INFURA_API_KEY = process.env.INFURA_API_KEY
const EVM_PRIVATE_KEY = process.env.EVM_PRIVATE_KEY || "11".repeat(32)
const ETHERSCAN_API_KEY = process.env.ETHERSCAN_API_KEY || ""
const ARBISCAN_API_KEY = process.env.ARBISCAN_API_KEY || ""
const BASESCAN_API_KEY = process.env.BASESCAN_API_KEY || ""


task("deploy-bytecode", "Deploys a contract with a given bytecode")
  .addParam("bytecode", "The path to the file containing the bytecode of the contract")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre

    const bytecode = fs.readFileSync(taskArgs.bytecode, "utf8")
    const [signer] = await ethers.getSigners()

    const contractFactory = new ethers.ContractFactory([], bytecode, signer)
    const contract = await contractFactory.deploy()
    await contract.waitForDeployment()

    console.log(
      JSON.stringify({
        contractAddress: await contract.getAddress(),
      }),
    )
  })

task("deploy-test-token", "Deploys the E2ETestToken contract")
  .addParam("name", "Token name")
  .addParam("symbol", "Token symbol")
  .addOptionalParam("supply", "Initial supply of tokens (default: 1000000 tokens)", "1000000")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre;

    // Convert supply to tokens with 18 decimals
    const supply = ethers.parseEther(taskArgs.supply);

    const [deployer] = await ethers.getSigners();

    const TestToken = await ethers.getContractFactory("E2ETestToken");
    const token = await TestToken.deploy(taskArgs.name, taskArgs.symbol, supply);
    await token.waitForDeployment();

    const tokenAddress = await token.getAddress();

    console.log(JSON.stringify({
      contractAddress: tokenAddress,
      name: taskArgs.name,
      symbol: taskArgs.symbol,
      supply: taskArgs.supply,
    }));
  });

task("mint-test-token", "Mints tokens to a specified address")
  .addParam("contract", "The address of the deployed token contract")
  .addParam("to", "The address to mint tokens to")
  .addParam("amount", "Amount of tokens to mint")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre;

    const amount = ethers.parseEther(taskArgs.amount);

    const [signer] = await ethers.getSigners();

    const token = await ethers.getContractAt("E2ETestToken", taskArgs.contract, signer);

    const tx = await token.mint(taskArgs.to, amount);
    await tx.wait();

    console.log(JSON.stringify({
      success: true,
      contractAddress: taskArgs.contract,
      to: taskArgs.to,
      amount: taskArgs.amount
    }));
  });

task("get-current-eoa", "Gets the EOA address of a current signer")
  .setAction(async (_, hre) => {
    const { ethers } = hre;

    const wallet = new ethers.Wallet(EVM_PRIVATE_KEY);
    console.log(JSON.stringify({
      address: wallet.address,
      privateKey: wallet.privateKey,
    }, null, 2));
  });

task("create-eoa", "Creates a new EOA account and prints its credentials")
  .setAction(async (_, hre) => {
    const { ethers } = hre;

    const wallet = ethers.Wallet.createRandom();

    console.log(JSON.stringify({
      address: wallet.address,
      privateKey: wallet.privateKey,
      mnemonic: wallet.mnemonic?.phrase
    }, null, 2));
  });


const config: HardhatUserConfig = {
  paths: {
    sources: "./src",
    cache: "./cache",
    artifacts: "./build",
    tests: "./tests",
  },
  solidity: {
    compilers: [
      {
        version: "0.8.24",
        settings: {
          optimizer: {
            enabled: true,
            runs: 200,
          },
          metadata: {
            // do not include the metadata hash, since this is machine dependent
            // and we want all generated code to be deterministic
            // https://docs.soliditylang.org/en/v0.8.24/metadata.html
            bytecodeHash: "none",
          },
        },
      },
    ],
  },
  networks: {
    hardhat: {
      chainId: 1337,
      mining: {
        auto: true,
        interval: 0,
      },
    },
    sepolia: {
      omniChainId: 0,
      chainId: 11155111,
      url: `https://sepolia.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    arbitrumSepolia: {
      wormholeAddress: "0x6b9C8671cdDC8dEab9c719bB87cBd3e782bA6a35",
      omniChainId: 3,
      chainId: 421614,
      url: `https://arbitrum-sepolia.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    baseSepolia: {
      wormholeAddress: "0x79A1027a6A159502049F10906D333EC57E95F083",
      omniChainId: 4,
      chainId: 84532,
      url: `https://base-sepolia.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
  },
  etherscan: {
    apiKey: {
      mainnet: ETHERSCAN_API_KEY,
      arbitrumMainnet: ARBISCAN_API_KEY,
      baseMainnet: BASESCAN_API_KEY,
      sepolia: ETHERSCAN_API_KEY,
      arbitrumSepolia: ARBISCAN_API_KEY,
      baseSepolia: BASESCAN_API_KEY,
    },
  },
}

export default config
