import "@nomicfoundation/hardhat-chai-matchers"
import "@nomicfoundation/hardhat-ethers"
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

import { getProxyImplementationAddress } from "./utils/zksync"
import "@matterlabs/hardhat-zksync"

declare module "hardhat/types/config" {
  interface HttpNetworkUserConfig {
    omniChainId: number
    wormholeAddress?: string
    zksync?: boolean
    ethNetwork?: string
  }
  interface HardhatUserConfig {
    zksolc?: {
      version?: string
      settings?: Record<string, unknown>
    }
    etherscan?: {
      apiKey?: string | Record<string, string>
      customChains?: Array<{
        network: string
        chainId: number
        urls: {
          apiURL: string
          browserURL: string
        }
      }>
    }
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
    const implementationAddress = await getProxyImplementationAddress(hre, bridgeAddress)

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

task(
  "deploy-hl-token-impl",
  "Deploys the HlBridgeToken (HyperliquedBridgeToken) implementation",
).setAction(async (_, hre) => {
  const { ethers } = hre
  const HlBridgeTokenContractFactory = await ethers.getContractFactory("HyperliquedBridgeToken")
  const HlBridgeTokenContract = await HlBridgeTokenContractFactory.deploy()
  await HlBridgeTokenContract.waitForDeployment()
  console.log(
    JSON.stringify({
      tokenImplAddress: await HlBridgeTokenContract.getAddress(),
    }),
  )
})

task(
  "deploy-hl-bridge-token-proxy",
  "Deploy an ERC1967Proxy over HyperliquedBridgeToken (5-arg initialize: name, symbol, decimals, systemAddress, hyperCoreDeployer)",
)
  .addParam("impl", "HyperliquedBridgeToken implementation address")
  .addParam("name", "Token name (e.g. 'NEAR')")
  .addParam("symbol", "Token symbol (e.g. 'NEAR')")
  .addParam("decimals", "Token decimals (e.g. '18')")
  .addParam("systemAddress", "HyperCore system address for this token (e.g. 0x20...0201)")
  .addParam(
    "hyperCoreDeployer",
    "Address of the HyperCore deployer (to be stored at keccak256('HyperCore deployer'))",
  )
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const impl = ethers.getAddress(taskArgs.impl)
    const systemAddress = ethers.getAddress(taskArgs.systemAddress)
    const hyperCoreDeployer = ethers.getAddress(taskArgs.hyperCoreDeployer)

    const HlBridgeToken = await ethers.getContractFactory("HyperliquedBridgeToken")
    // `initialize` is overloaded (inherited 3-arg from BridgeToken + 5-arg from HyperliquedBridgeToken),
    // so we must pass the full canonical signature to disambiguate.
    const initData = HlBridgeToken.interface.encodeFunctionData(
      "initialize(string,string,uint8,address,address)",
      [
        taskArgs.name,
        taskArgs.symbol,
        Number.parseInt(taskArgs.decimals, 10),
        systemAddress,
        hyperCoreDeployer,
      ],
    )

    const ProxyFactory = await ethers.getContractFactory("ERC1967Proxy")
    const proxy = await ProxyFactory.deploy(impl, initData)
    await proxy.waitForDeployment()
    const proxyAddress = await proxy.getAddress()

    const EIP1967_IMPL_SLOT = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
    const implSlot = await ethers.provider.getStorage(proxyAddress, EIP1967_IMPL_SLOT)
    const implementation = ethers.getAddress(`0x${implSlot.slice(-40)}`)

    console.log(
      JSON.stringify(
        {
          proxyAddress,
          implementation,
          name: taskArgs.name,
          symbol: taskArgs.symbol,
          decimals: Number.parseInt(taskArgs.decimals, 10),
          systemAddress,
          hyperCoreDeployer,
        },
        null,
        2,
      ),
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

    const currentImpl = await getProxyImplementationAddress(hre, taskArgs.factory)
    await upgrades.upgradeProxy(taskArgs.factory, OmniBridgeContract)
    const newImpl = await getProxyImplementationAddress(hre, taskArgs.factory)

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

task(
  "set-hyper-core-deployer",
  "Set the HyperCore deployer address stored at slot keccak256('HyperCore deployer') (onlyOwner)",
)
  .addParam("token", "Token proxy address (HlBridgeToken / HyperliquedBridgeToken)")
  .addParam("deployer", "HyperCore deployer address to write into the namespaced slot")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const [signer] = await ethers.getSigners()
    const proxy = ethers.getAddress(taskArgs.token)
    const deployer = ethers.getAddress(taskArgs.deployer)

    const HYPER_CORE_DEPLOYER_SLOT = ethers.keccak256(ethers.toUtf8Bytes("HyperCore deployer"))

    // Read the current value via direct storage slot (works even if the contract
    // doesn't expose a getter — slot is canonical).
    const beforeRaw = await ethers.provider.getStorage(proxy, HYPER_CORE_DEPLOYER_SLOT)
    const before = ethers.getAddress(`0x${beforeRaw.slice(-40)}`)

    const iface = new ethers.Interface([
      "function setHyperCoreDeployer(address) external",
      "function owner() external view returns (address)",
    ])
    const token = new ethers.Contract(proxy, iface, signer)

    const owner: string = await token.owner()
    const signerAddress = await signer.getAddress()
    if (signerAddress.toLowerCase() !== owner.toLowerCase()) {
      console.warn(
        `WARNING: signer (${signerAddress}) is not the owner (${owner}) — tx will revert`,
      )
    }

    const tx = await token.setHyperCoreDeployer(deployer)
    const receipt = await tx.wait()

    const afterRaw = await ethers.provider.getStorage(proxy, HYPER_CORE_DEPLOYER_SLOT)
    const after = ethers.getAddress(`0x${afterRaw.slice(-40)}`)

    console.log(
      JSON.stringify(
        {
          proxy,
          signer: signerAddress,
          owner,
          slot: HYPER_CORE_DEPLOYER_SLOT,
          hyperCoreDeployerBefore: before,
          hyperCoreDeployerAfter: after,
          txHash: receipt.hash,
        },
        null,
        2,
      ),
    )
  })

task(
  "transfer-token-ownership",
  "Step 1 of Ownable2Step: current owner initiates ownership transfer (sets pendingOwner = newOwner). The new owner must then call acceptOwnership themselves.",
)
  .addParam("token", "Proxy address of the token")
  .addParam("newOwner", "Address that will become the new owner after they call acceptOwnership")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const [signer] = await ethers.getSigners()
    const proxy = ethers.getAddress(taskArgs.token)
    const newOwner = ethers.getAddress(taskArgs.newOwner)

    const token = new ethers.Contract(
      proxy,
      [
        "function transferOwnership(address) external",
        "function owner() external view returns (address)",
        "function pendingOwner() external view returns (address)",
      ],
      signer,
    )

    const ownerBefore: string = await token.owner()
    const pendingBefore: string = await token.pendingOwner()
    const signerAddress = await signer.getAddress()

    if (signerAddress.toLowerCase() !== ownerBefore.toLowerCase()) {
      console.warn(
        `WARNING: signer (${signerAddress}) is not the current owner (${ownerBefore}) — tx will revert`,
      )
    }

    const tx = await token.transferOwnership(newOwner)
    const receipt = await tx.wait()
    const ownerAfter: string = await token.owner()
    const pendingAfter: string = await token.pendingOwner()

    console.log(
      JSON.stringify(
        {
          proxy,
          signer: signerAddress,
          ownerBefore,
          pendingOwnerBefore: pendingBefore,
          ownerAfter,
          pendingOwnerAfter: pendingAfter,
          txHash: receipt.hash,
          nextStep: `New owner (${newOwner}) must call acceptOwnership() on ${proxy} to complete the transfer.`,
        },
        null,
        2,
      ),
    )
  })

task(
  "inspect-token",
  "Inspect a token contract: detect proxy, read implementation, ERC-20 metadata, owner, and HL-specific fields",
)
  .addParam("token", "Contract address to inspect")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre
    const provider = ethers.provider
    const addr = ethers.getAddress(taskArgs.token)

    const result: Record<string, unknown> = { address: addr }

    // 0. Is there code at all?
    const code = await provider.getCode(addr)
    result.isContract = code !== "0x"
    result.codeSize = (code.length - 2) / 2 // bytes
    if (!result.isContract) {
      console.log(JSON.stringify(result, null, 2))
      return
    }

    // 1. EIP-1967 proxy detection: implementation slot, admin slot, beacon slot
    const EIP1967_IMPL_SLOT = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
    const EIP1967_ADMIN_SLOT = "0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103"
    const EIP1967_BEACON_SLOT = "0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50"
    const implSlot = await provider.getStorage(addr, EIP1967_IMPL_SLOT)
    const adminSlot = await provider.getStorage(addr, EIP1967_ADMIN_SLOT)
    const beaconSlot = await provider.getStorage(addr, EIP1967_BEACON_SLOT)
    const implementation = ethers.getAddress(`0x${implSlot.slice(-40)}`)
    const admin = ethers.getAddress(`0x${adminSlot.slice(-40)}`)
    const beacon = ethers.getAddress(`0x${beaconSlot.slice(-40)}`)
    result.isProxy = implementation !== ethers.ZeroAddress
    result.implementation = implementation
    result.proxyAdmin = admin === ethers.ZeroAddress ? null : admin
    result.beacon = beacon === ethers.ZeroAddress ? null : beacon

    // 2. ERC-20 + Ownable2Step + UUPS reads, each wrapped in try/catch
    const iface = new ethers.Interface([
      "function name() view returns (string)",
      "function symbol() view returns (string)",
      "function decimals() view returns (uint8)",
      "function totalSupply() view returns (uint256)",
      "function owner() view returns (address)",
      "function pendingOwner() view returns (address)",
      "function UPGRADE_INTERFACE_VERSION() view returns (string)",
      "function proxiableUUID() view returns (bytes32)",
      "function systemAddress() view returns (address)",
    ])
    const c = new ethers.Contract(addr, iface, provider)

    const calls: Array<[string, () => Promise<unknown>]> = [
      ["name", () => c.name()],
      ["symbol", () => c.symbol()],
      ["decimals", () => c.decimals()],
      ["totalSupply", () => c.totalSupply()],
      ["owner", () => c.owner()],
      ["pendingOwner", () => c.pendingOwner()],
      ["UPGRADE_INTERFACE_VERSION", () => c.UPGRADE_INTERFACE_VERSION()],
      ["proxiableUUID", () => c.proxiableUUID()],
      ["systemAddress", () => c.systemAddress()],
    ]
    for (const [key, fn] of calls) {
      try {
        const v = await fn()
        result[key] = typeof v === "bigint" ? v.toString() : v
      } catch {
        result[key] = null
      }
    }

    // 3. HyperCore-deployer slot (keccak256("HyperCore deployer"))
    const HYPER_CORE_DEPLOYER_SLOT = ethers.keccak256(ethers.toUtf8Bytes("HyperCore deployer"))
    const hcSlot = await provider.getStorage(addr, HYPER_CORE_DEPLOYER_SLOT)
    const hcDeployer = ethers.getAddress(`0x${hcSlot.slice(-40)}`)
    result.hyperCoreDeployer = hcDeployer === ethers.ZeroAddress ? null : hcDeployer

    console.log(JSON.stringify(result, null, 2))
  })

const config: HardhatUserConfig = {
  zksolc: {
    version: "1.5.15",
    settings: {
      // Note: This must be true to call NonceHolder & ContractDeployer system contracts
      enableEraVMExtensions: true,
      // Use evmla codegen to match solc's default behavior and avoid unexpected differences
      codegen: "evmla",
    },
  },
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
    hyperEvmMainnet: {
      wormholeAddress: "0x7C0faFc4384551f063e05aee704ab943b8B53aB3",
      omniChainId: 9,
      chainId: 999,
      url: "https://rpc.hyperliquid.xyz/evm",
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    abstractMainnet: {
      omniChainId: 11,
      chainId: 2741,
      url: "https://api.mainnet.abs.xyz",
      ethNetwork: "mainnet",
      zksync: true,
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
      url: "https://rpcs.chain.link/hyperevm/testnet",
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
    abstractTestnet: {
      omniChainId: 11,
      chainId: 11124,
      url: "https://api.testnet.abs.xyz",
      ethNetwork: "sepolia",
      zksync: true,
      accounts: [`${EVM_PRIVATE_KEY}`],
    },
  },
  etherscan: {
    apiKey: ETHERSCAN_API_KEY,
    customChains: [
      {
        network: "abstractTestnet",
        chainId: 11124,
        urls: {
          apiURL: "https://api.etherscan.io/v2/api?chainid=11124",
          browserURL: "https://sepolia.abscan.org",
        },
      },
      {
        network: "abstractMainnet",
        chainId: 2741,
        urls: {
          apiURL: "https://api.etherscan.io/v2/api?chainid=2741",
          browserURL: "https://abscan.org",
        },
      },
    ],
  },
}

export default config
