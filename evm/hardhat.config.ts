import "@nomicfoundation/hardhat-chai-matchers"
import "@nomicfoundation/hardhat-ethers"
import "@nomicfoundation/hardhat-verify"
import "@openzeppelin/hardhat-upgrades"
import "@typechain/hardhat"
import * as dotenv from "dotenv"
import "hardhat-storage-layout"
import type { HardhatUserConfig } from "hardhat/config"
import "solidity-coverage"
import "./src/eNear/scripts"
import { task } from "hardhat/config"
import type { HttpNetworkUserConfig } from "hardhat/types"
import type { OmniBridge, OmniBridgeWormhole } from "./typechain-types"
import { deriveEVMAddress, mpcRootPublicKeys } from "./utils/kdf"

import "hardhat/types/config"
import assert from "node:assert"
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
  .addParam("nearBridgeAccountId", "The OmniBridge account ID on NEAR")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre
    const networkConfig = hre.network.config as HttpNetworkUserConfig
    const omniChainId = networkConfig.omniChainId
    const wormholeAddress = networkConfig.wormholeAddress

    const mpcRootPublicKey = hre.network.name.toLowerCase().endsWith("mainnet")
      ? mpcRootPublicKeys.mainnet.key
      : mpcRootPublicKeys.testnet.key

    const nearBridgeDerivedAddress = await deriveEVMAddress(
      taskArgs.nearBridgeAccountId,
      "bridge-1",
      mpcRootPublicKey,
    )

    const isWormholeContract = wormholeAddress ?? false
    const contractName = isWormholeContract ? "OmniBridgeWormhole" : "OmniBridge"
    const OmniBridgeContract = await ethers.getContractFactory(contractName)
    const consistencyLevel = 0

    const OmniBridge = await upgrades.deployProxy(
      OmniBridgeContract,
      isWormholeContract
        ? [
            taskArgs.bridgeTokenImpl,
            nearBridgeDerivedAddress,
            omniChainId,
            wormholeAddress,
            consistencyLevel,
          ]
        : [taskArgs.bridgeTokenImpl, nearBridgeDerivedAddress, omniChainId],
      {
        initializer: isWormholeContract ? "initializeWormhole" : "initialize",
        timeout: 0,
      },
    )

    await OmniBridge.waitForDeployment()
    const bridgeAddress = await OmniBridge.getAddress()
    const implementationAddress = await upgrades.erc1967.getImplementationAddress(bridgeAddress)

    const wormholeAddressStorageValue = await hre.ethers.provider.getStorage(bridgeAddress, 58)
    const decodedWormholeAddress = ethers.AbiCoder.defaultAbiCoder().decode(
      ["address"],
      wormholeAddressStorageValue,
    )[0]
    assert.strictEqual(decodedWormholeAddress, wormholeAddress ?? ethers.ZeroAddress)

    console.log(
      JSON.stringify({
        bridgeAddress,
        implementationAddress,
        derivedAddress: nearBridgeDerivedAddress,
        omniChainId,
        wormholeAddress: wormholeAddress ?? null,
      }),
    )
  })

task("deploy-token-factory-impl", "Deploys the BridgeToken Factory implementation").setAction(
  async (_, hre) => {
    const { ethers } = hre
    const OmniBridgeContractFactory = await ethers.getContractFactory("OmniBridge")
    const OmniBridgeContract = await OmniBridgeContractFactory.deploy()
    await OmniBridgeContract.waitForDeployment()
    console.log(
      JSON.stringify({
        tokenImplAddress: await OmniBridgeContract.getAddress(),
      }),
    )
  },
)

task("deploy-token-impl", "Deploys the BridgeToken implementation").setAction(async (_, hre) => {
  const { ethers } = hre
  const BridgeTokenContractFactory = await ethers.getContractFactory("BridgeToken")
  const BridgeTokenContract = await BridgeTokenContractFactory.deploy()
  await BridgeTokenContract.waitForDeployment()
  console.log(
    JSON.stringify({
      tokenImplAddress: await BridgeTokenContract.getAddress(),
    }),
  )
})

task("upgrade-bridge-token", "Upgrades a BridgeToken to a new implementation")
  .addParam("factory", "The address of the OmniBridge contract")
  .addParam("nearTokenAccount", "The NEAR token ID")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre

    const OmniBridgeContract = await ethers.getContractFactory("OmniBridge")
    const OmniBridge = OmniBridgeContract.attach(taskArgs.factory) as OmniBridge

    const BridgeTokenV2Instance = await ethers.getContractFactory("BridgeTokenV2")
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy()
    await BridgeTokenV2.waitForDeployment()

    console.log(`BridgeTokenV2 deployed at ${await BridgeTokenV2.getAddress()}`)

    const tx = await OmniBridge.upgradeToken(
      taskArgs.nearTokenAccount,
      await BridgeTokenV2.getAddress(),
    )
    await tx.wait()

    console.log(
      JSON.stringify({
        upgradingToken: taskArgs.nearTokenAccount,
        tokenProxyAddress: await OmniBridge.nearToEthToken(taskArgs.nearTokenAccount),
        newImplementationAddress: await BridgeTokenV2.getAddress(),
      }),
    )
  })

task("upgrade-factory", "Upgrades the OmniBridge contract")
  .addParam("factory", "The address of the OmniBridge contract")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre
    const networkConfig = hre.network.config as HttpNetworkUserConfig
    const wormholeAddress = networkConfig.wormholeAddress
    const isWormholeContract = wormholeAddress ?? false
    const contractName = isWormholeContract ? "OmniBridgeWormhole" : "OmniBridge"

    const OmniBridgeContract = await ethers.getContractFactory(contractName)

    const currentImpl = await upgrades.erc1967.getImplementationAddress(taskArgs.factory)
    await upgrades.upgradeProxy(taskArgs.factory, OmniBridgeContract)
    const newImpl = await upgrades.erc1967.getImplementationAddress(taskArgs.factory)

    console.log(
      JSON.stringify({
        proxyAddress: taskArgs.factory,
        previousImplementation: currentImpl,
        newImplementation: newImpl,
      }),
    )
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

task("update-wormhole-address", "Update the wormhole address")
  .addParam("factory", "The address of the OmniBridge contract")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const networkConfig = hre.network.config as HttpNetworkUserConfig
    const wormholeAddress = networkConfig.wormholeAddress
    if (!wormholeAddress) {
      throw new Error("Wormhole address is not set")
    }

    const OmniBridgeContract = await ethers.getContractFactory("OmniBridgeWormhole")
    const consistencyLevel = 0
    const OmniBridge = OmniBridgeContract.attach(taskArgs.factory) as OmniBridgeWormhole
    const tx = await OmniBridge.setWormholeAddress(wormholeAddress, consistencyLevel)
    const receipt = await tx.wait()

    console.log("Address upgraded at tx hash:", receipt?.hash)
  })

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
    mainnet: {
      omniChainId: 0,
      chainId: 1,
      url: `https://mainnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    arbitrumMainnet: {
      wormholeAddress: "0xa5f208e072434bC67592E4C49C1B991BA79BCA46",
      omniChainId: 3,
      chainId: 42161,
      url: `https://arbitrum-mainnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    baseMainnet: {
      wormholeAddress: "0xbebdb6C8ddC678FfA9f8748f85C815C556Dd8ac6",
      omniChainId: 4,
      chainId: 8453,
      url: `https://base-mainnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    bnbMainnet: {
      wormholeAddress: "0x98f3c9e6E3fAce36bAAd05FE09d375Ef1464288B",
      omniChainId: 5,
      chainId: 56,
      url: `https://bsc-mainnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    polygonMainnet: {
      wormholeAddress: "0x7A4B5a56256163F07b2C80A7cA55aBE66c4ec4d7",
      omniChainId: 8,
      chainId: 137,
      url: `https://polygon-mainnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
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
    bnbTestnet: {
      wormholeAddress: "0x68605AD7b15c732a30b1BbC62BE8F2A509D74b4D",
      omniChainId: 5,
      chainId: 97,
      url: `https://bsc-testnet.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    polygonAmoy: {
      wormholeAddress: "0x6b9C8671cdDC8dEab9c719bB87cBd3e782bA6a35",
      omniChainId: 8,
      chainId: 80002,
      url: `https://polygon-amoy.infura.io/v3/${INFURA_API_KEY}`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    hyperEvmTestnet: {
      wormholeAddress: "0xBB73cB66C26740F31d1FabDC6b7A46a038A300dd",
      omniChainId: 9,
      chainId: 998,
      url: `https://rpcs.chain.link/hyperevm/testnet`,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
  },
  etherscan: {
    apiKey: ETHERSCAN_API_KEY,
  },
}

export default config
