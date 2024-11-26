import "@nomicfoundation/hardhat-chai-matchers"
import "@nomicfoundation/hardhat-ethers"
import "@nomicfoundation/hardhat-verify"
import "@openzeppelin/hardhat-upgrades"
import "@typechain/hardhat"
import * as dotenv from "dotenv"
import "hardhat-storage-layout"
import { type HardhatUserConfig, task } from "hardhat/config"
import "solidity-coverage"
import "./src/eNear/scripts"
import type { OmniBridge } from "./typechain-types"

dotenv.config()

const EVM_RPC_URL = process.env.INFURA_API_KEY
const EVM_PRIVATE_KEY = process.env.ETH_PRIVATE_KEY || "11".repeat(32)
const ETHERSCAN_API_KEY = process.env.ETHERSCAN_API_KEY

task("set-metadata-ft", "Set metadata for NEP-141 tokens on the Ethereum side")
  .addParam("nearTokenAccount", "Near account id of the token")
  .addParam("name", "The new name of the token")
  .addParam("symbol", "The new symbol of the token")
  .addParam("factory", "The address of the factory contract on Ethereum")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const OmniBridgeContract = await ethers.getContractFactory("OmniBridge")
    const OmniBridge = OmniBridgeContract.attach(taskArgs.factory) as OmniBridge
    await OmniBridge.setMetadata(taskArgs.nearTokenAccount, taskArgs.name, taskArgs.symbol)
  })

task("deploy-bridge-token-factory", "Deploys the OmniBridge contract")
  .addParam("bridgeTokenImpl", "The address of the bridge token implementation")
  .addParam("nearBridgeDerivedAddress", "The derived EVM address of the Near's OmniBridge")
  .addParam("omniBridgeChainId", "Chain Id of the network in the OmniBridge")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre

    const OmniBridgeContract = await ethers.getContractFactory("OmniBridge")
    const OmniBridge = await upgrades.deployProxy(
      OmniBridgeContract,
      [taskArgs.bridgeTokenImpl, taskArgs.nearBridgeDerivedAddress, taskArgs.omniBridgeChainId],
      {
        initializer: "initialize",
        timeout: 0,
      },
    )

    await OmniBridge.waitForDeployment()
    console.log(`OmniBridge deployed at ${await OmniBridge.getAddress()}`)
    console.log(
      "Implementation address:",
      await upgrades.erc1967.getImplementationAddress(await OmniBridge.getAddress()),
    )
  })

task("deploy-token-impl", "Deploys the BridgeToken implementation").setAction(async (_, hre) => {
  const { ethers } = hre

  const BridgeTokenContractFactory = await ethers.getContractFactory("BridgeToken")
  const BridgeTokenContract = await BridgeTokenContractFactory.deploy()
  await BridgeTokenContract.waitForDeployment()
  console.log(`BridgeTokenContract deployed at ${await BridgeTokenContract.getAddress()}`)
})

task("upgrade-bridge-token", "Upgrades a BridgeToken to a new implementation")
  .addParam("factory", "The address of the OmniBridge contract")
  .addParam("nearTokenAccount", "The NEAR token ID")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre

    const OmniBridgeContract = await ethers.getContractFactory("OmniBridge")
    const OmniBridge = OmniBridgeContract.attach(taskArgs.factory) as OmniBridge

    console.log(`Upgrading token ${taskArgs.nearTokenAccount}`)
    console.log("Token proxy address:", await OmniBridge.nearToEthToken(taskArgs.nearTokenAccount))

    const BridgeTokenV2Instance = await ethers.getContractFactory("BridgeTokenV2")
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy()
    await BridgeTokenV2.waitForDeployment()

    console.log(`BridgeTokenV2 deployed at ${await BridgeTokenV2.getAddress()}`)

    const tx = await OmniBridge.upgradeToken(
      taskArgs.nearTokenAccount,
      await BridgeTokenV2.getAddress(),
    )
    const receipt = await tx.wait()

    console.log("Token upgraded at tx hash:", receipt?.hash)
  })

task("upgrade-factory", "Upgrades the OmniBridge contract")
  .addParam("factory", "The address of the OmniBridge contract")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre

    const OmniBridgeContract = await ethers.getContractFactory("OmniBridge")
    console.log(
      "Current implementation address:",
      await upgrades.erc1967.getImplementationAddress(taskArgs.factory),
    )
    console.log("Upgrade factory, proxy address", taskArgs.factory)
    await upgrades.upgradeProxy(taskArgs.factory, OmniBridgeContract)
  })

task("etherscan-verify", "Verify contract on etherscan")
  .addParam("address", "Contract address")
  .addParam("args", "Constructor arguments in JSON array")
  .setAction(async (taskArgs, hre) => {
    await hre.run("verify:verify", {
      address: taskArgs.address,
      constructorArguments: JSON.parse(taskArgs.args),
    })
  })

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
    eth_mainnet: {
      chainId: 1,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    base_mainnet: {
      chainId: 8453,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    arb_mainnet: {
      chainId: 42161,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    eth_sepolia: {
      chainId: 11155111,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    base_sepolia: {
      chainId: 84532,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    arb_sepolia: {
      chainId: 421614,
      url: EVM_RPC_URL,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
  },
  etherscan: {
    apiKey: ETHERSCAN_API_KEY,
  },
}

export default config
