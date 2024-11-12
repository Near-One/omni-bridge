require("dotenv").config();
require("@openzeppelin/hardhat-upgrades");
require("@nomicfoundation/hardhat-verify");
require("@nomicfoundation/hardhat-chai-matchers");

const ALCHEMY_API_KEY = process.env.ALCHEMY_API_KEY;
const INFURA_API_KEY = process.env.INFURA_API_KEY;
const ETH_PRIVATE_KEY = process.env.ETH_PRIVATE_KEY || '11'.repeat(32);
const ETHERSCAN_API_KEY = process.env.ETHERSCAN_API_KEY;

task('set-metadata-ft', 'Set metadata for NEP-141 tokens on the Ethereum side')
  .addParam('nearTokenAccount', 'Near account id of the token')
  .addParam('name', 'The new name of the token')
  .addParam('symbol', 'The new symbol of the token')
  .addParam('factory', 'The address of the factory contract on Ethereum')
  .setAction(async (taskArgs) => {
    const BridgeTokenFactoryContract = await ethers.getContractFactory("BridgeTokenFactory");
    const BridgeTokenFactory = BridgeTokenFactoryContract.attach(taskArgs.factory);
    await BridgeTokenFactory.setMetadata(taskArgs.nearTokenAccount, taskArgs.name, taskArgs.symbol);
  });

task('add-token-to-whitelist-eth', 'Add a token to whitelist')
  .addParam('nearTokenAccount', 'Near account id of the token')
  .addParam('factory', 'The address of the eth factory contract')
  .addParam('mode', 'Whitelist mode: [ NotInitialized, Blocked, CheckToken, CheckAccountAndToken ]')
  .setAction(async (taskArgs) => {
    const WhitelistMode = {
      NotInitialized: 0,
      Blocked: 1,
      CheckToken: 2,
      CheckAccountAndToken: 3
    }
    const BridgeTokenFactoryContract = await ethers.getContractFactory("BridgeTokenFactory");
    const BridgeTokenFactory = BridgeTokenFactoryContract.attach(taskArgs.factory);
    const tx = await BridgeTokenFactory.setTokenWhitelistMode(taskArgs.nearTokenAccount, WhitelistMode[taskArgs.mode]);
    const receipt = await tx.wait();
    console.log("Tx hash", receipt.transactionHash);
  });

task('add-account-to-whitelist-eth', 'Add an account to whitelist')
  .addParam('nearTokenAccount', 'Near account id of the token')
  .addParam('ethAccount', 'Ethereum account address to add to whitelist')
  .addParam('factory', 'The address of the factory contract on Ethereum')
  .setAction(async (taskArgs) => {
    const BridgeTokenFactoryContract = await ethers.getContractFactory("BridgeTokenFactory");
    const BridgeTokenFactory = BridgeTokenFactoryContract.attach(taskArgs.factory);
    const tx = await BridgeTokenFactory.addAccountToWhitelist(taskArgs.nearTokenAccount, taskArgs.ethAccount);
    const receipt = await tx.wait();
    console.log("Tx hash", receipt.transactionHash);
  });

task("deploy-bridge-token-factory", "Deploys the BridgeTokenFactory contract")
  .addParam("bridgeTokenImpl", "The address of the bridge token implementation")
  .addParam("nearBridgeDerivedAddress", "The derived EVM address of the Near's OmniBridge")
  .addParam("omniBridgeChainId", "Chain Id of the network in the OmniBridge")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre;

    const BridgeTokenFactoryContract = 
      await ethers.getContractFactory("BridgeTokenFactory");
    const BridgeTokenFactory = await upgrades.deployProxy(
      BridgeTokenFactoryContract,
      [
        taskArgs.bridgeTokenImpl,
        taskArgs.nearBridgeDerivedAddress,
        taskArgs.omniBridgeChainId
      ],
      {
        initializer: "initialize",
        timeout: 0,
      },
    );

    await BridgeTokenFactory.waitForDeployment();
    console.log(`BridgeTokenFactory deployed at ${await BridgeTokenFactory.getAddress()}`);
    console.log(
      "Implementation address:",
      await upgrades.erc1967.getImplementationAddress(
        await BridgeTokenFactory.getAddress(),
      ),
    );
  });

task("deploy-token-impl", "Deploys the BridgeToken implementation")
  .setAction(async () => {
    const { ethers } = hre;

    const BridgeTokenContractFactory = await ethers.getContractFactory("BridgeToken");
    const BridgeTokenContract = await BridgeTokenContractFactory.deploy();
    await BridgeTokenContract.waitForDeployment();
    console.log(`BridgeTokenContract deployed at ${await BridgeTokenContract.getAddress()}`);
  });

task("upgrade-bridge-token", "Upgrades a BridgeToken to a new implementation")
  .addParam("factory", "The address of the BridgeTokenFactory contract")
  .addParam("nearTokenAccount", "The NEAR token ID")
  .setAction(async (taskArgs, hre) => {
    const { ethers } = hre;

    const BridgeTokenFactoryContract = await ethers.getContractFactory("BridgeTokenFactory");
    const BridgeTokenFactory = BridgeTokenFactoryContract.attach(taskArgs.factory);

    console.log(`Upgrading token ${taskArgs.nearTokenAccount}`);
    console.log(`Token proxy address:`, await BridgeTokenFactory.nearToEthToken(taskArgs.nearTokenAccount));

    const BridgeTokenV2Instance = await ethers.getContractFactory("BridgeTokenV2");
    const BridgeTokenV2 = await BridgeTokenV2Instance.deploy();
    await BridgeTokenV2.waitForDeployment();

    console.log(`BridgeTokenV2 deployed at ${await BridgeTokenV2.getAddress()}`);

    const tx = await BridgeTokenFactory.upgradeToken(taskArgs.nearTokenAccount, await BridgeTokenV2.getAddress());
    const receipt = await tx.wait();

    console.log("Token upgraded at tx hash:", receipt.transactionHash);
  });

task("upgrade-factory", "Upgrades the BridgeTokenFactory contract")
  .addParam("factory", "The address of the BridgeTokenFactory contract")
  .setAction(async (taskArgs, hre) => {
    const { ethers, upgrades } = hre;

    const BridgeTokenFactoryContract = await ethers.getContractFactory("BridgeTokenFactory");
    console.log("Current implementation address:", await upgrades.erc1967.getImplementationAddress(taskArgs.factory));
    console.log("Upgrade factory, proxy address", taskArgs.factory);
    await upgrades.upgradeProxy(taskArgs.factory, BridgeTokenFactoryContract);
  });

task('etherscan-verify', 'Verify contract on etherscan')
  .addParam('address', 'Contract address')
  .addParam('args', 'Constructor arguments in JSON array')
  .setAction(async (taskArgs, hre) => {
    await hre.run("verify:verify", {
      address: taskArgs.address,
      constructorArguments: JSON.parse(taskArgs.args),
    });
  });

module.exports = {
  paths: {
    sources: './src',
    cache: "./cache",
    artifacts: './build',
    tests: './tests'
  },
  solidity: {
    compilers: [
      {
        version: '0.8.24',
        settings: {
          optimizer: {
            enabled: true,
            runs: 200
          },
          metadata: {
            // do not include the metadata hash, since this is machine dependent
            // and we want all generated code to be deterministic
            // https://docs.soliditylang.org/en/v0.8.24/metadata.html
            bytecodeHash: "none"
          }
        }
      }
    ]
  },
  networks: {
    sepolia: {
      url: INFURA_API_KEY
        ? `https://sepolia.infura.io/v3/${INFURA_API_KEY}`
        : `https://eth-sepolia.g.alchemy.com/v2/${ALCHEMY_API_KEY}`,
      accounts: [`${ETH_PRIVATE_KEY}`]
    },
    mainnet: {
      url: INFURA_API_KEY
        ? `https://mainnet.infura.io/v3/${INFURA_API_KEY}`
        : `https://eth-mainnet.alchemyapi.io/v2/${ALCHEMY_API_KEY}`,
      accounts: [`${ETH_PRIVATE_KEY}`]
    },
  },
  etherscan: {
    apiKey: ETHERSCAN_API_KEY
  },
}
