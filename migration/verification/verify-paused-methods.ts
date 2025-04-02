import { ethers } from "ethers";
import { type Account, connect, keyStores } from "near-api-js";
import dotenv from "dotenv";
import chalk from "chalk";

// Load environment variables
dotenv.config();

// Define interfaces for configs
interface EthereumConfig {
	rpcUrl: string;
	contracts: {
		[key: string]: string;
	};
}

interface NearConfig {
	networkId: string;
	nodeUrl: string;
	contracts: {
		[key: string]: string;
	};
}

// ABIs for Ethereum contracts
const erc20LockerABI = [
	"function paused() view returns (uint256)",
	"function adminPause(uint256) external",
];

const ethCustodianABI = [
	"function paused() view returns (uint256)",
	"function pausedFlags() view returns (uint256)",
	"function adminPause(uint256) external",
];

const bridgeTokenFactoryABI = [
	"function paused() view returns (uint256)",
	"function pausedFlags() view returns (uint256)",
	"function pauseWithdraw() view returns (bool)",
];

const omniBridgeABI = [
	"function paused() view returns (uint256)",
	"function pausedFlags() view returns (uint256)",
];

const eNearABI = [
	"function paused() view returns (uint256)",
	"function pausedFlags() view returns (uint256)",
];

// Proxy interface for EIP-1967 Transparent Proxies
const proxyABI = ["function implementation() view returns (address)"];

// Main verification function
async function verifyPausedMethods(): Promise<void> {
	console.log(
		chalk.blue.bold(
			"Verifying pause status for all migration-related contracts...",
		),
	);

	// Setup Ethereum configuration
	const network = process.env.NETWORK_ETH || "sepolia";
	const rpcUrl =
		process.env.ETH_RPC_URL || `https://eth-${network}.public.blastapi.io`;

	const ethereumConfig: EthereumConfig = {
		rpcUrl,
		contracts: {
			// Main contracts
			omniBridge:
				process.env.OMNI_BRIDGE_ETH || process.env.OMNI_BRIDGE_ADDRESS || "",
			erc20Locker: process.env.ERC20_LOCKER || "",
			ethCustodianProxy: process.env.ETH_CUSTODIAN_PROXY || "",
			ethCustodian: process.env.ETH_CUSTODIAN || "",
			bridgeTokenFactory: process.env.BRIDGE_TOKEN_FACTORY || "",
			eNear: process.env.E_NEAR_ADDRESS || "",
		},
	};

	// Setup NEAR configuration
	const nearConfig: NearConfig = {
		networkId: process.env.NETWORK_NEAR || "testnet",
		nodeUrl: `https://rpc.${process.env.NETWORK_NEAR || "testnet"}.near.org`,
		contracts: {
			// Main contracts
			tokenLocker: process.env.TOKEN_LOCKER || "",
			eNearAccount: process.env.E_NEAR_ACCOUNT_ID || "",
			omniBridgeNear: process.env.OMNI_BRIDGE_ACCOUNT_ID || "",
			bridgeTokenFactoryNear: process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID || "",
			aurora: process.env.AURORA_ACCOUNT_ID || "aurora",
		},
	};

	// Verify Ethereum contracts
	console.log(chalk.cyan.bold("\n=== Ethereum Contracts ==="));
	await verifyEthereumContracts(ethereumConfig);

	// Verify NEAR contracts
	console.log(chalk.cyan.bold("\n=== NEAR Contracts ==="));
	await verifyNearContracts(nearConfig);

	// Special check for Aurora Engine precompiles
	console.log(chalk.cyan.bold("\n=== Aurora Engine Precompiles ==="));
	await checkAuroraPrecompiles(nearConfig);
}

async function verifyEthereumContracts(config: EthereumConfig): Promise<void> {
	const provider = new ethers.JsonRpcProvider(config.rpcUrl);

	// Check OmniBridge (new bridge)
	await checkContract(
		"OmniBridge",
		config.contracts.omniBridge,
		omniBridgeABI,
		provider,
		checkPausedFlags,
	);

	// Check ERC20 Locker (original Rainbow bridge)
	await checkContract(
		"ERC20 Locker",
		config.contracts.erc20Locker,
		erc20LockerABI,
		provider,
		checkPausedFlags,
	);

	// Check ETH Custodian Proxy
	await checkContract(
		"ETH Custodian Proxy",
		config.contracts.ethCustodianProxy,
		ethCustodianABI,
		provider,
		checkPausedFlags,
	);

	// Check Bridge Token Factory
	await checkContract(
		"Bridge Token Factory",
		config.contracts.bridgeTokenFactory,
		bridgeTokenFactoryABI,
		provider,
		checkPausedFlags,
	);

	// Check eNEAR on Ethereum
	await checkContract(
		"eNEAR",
		config.contracts.eNear,
		eNearABI,
		provider,
		checkPausedFlags,
	);
}

async function checkContract(
	name: string,
	address: string,
	abi: string[],
	provider: ethers.JsonRpcProvider,
	checkFn: (contract: ethers.Contract, name: string) => Promise<void>,
): Promise<void> {
	if (!address) {
		console.log(chalk.yellow(`${name} address not provided, skipping...`));
		return;
	}

	console.log(chalk.cyan(`\nChecking ${name} (${address}):`));

	try {
		// First check if this is a proxy contract
		// For EIP-1967 Transparent Proxies, we can get the implementation address
		let implementationAddress: string | null = null;

		try {
			const proxyContract = new ethers.Contract(address, proxyABI, provider);
			// Try to get implementation address
			implementationAddress = await proxyContract.implementation();
			console.log(
				chalk.yellow(
					`  Detected proxy contract with implementation at: ${implementationAddress}`,
				),
			);
		} catch (e) {
			// Not a proxy or doesn't expose implementation function

			// Try the EIP-1967 storage slot directly
			// The implementation slot is: 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
			try {
				const implementationSlot =
					"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";
				const storageValue = await provider.getStorage(
					address,
					implementationSlot,
				);

				// Convert the storage value to an address (remove leading zeros and add 0x prefix)
				if (
					storageValue &&
					storageValue !==
						"0x0000000000000000000000000000000000000000000000000000000000000000"
				) {
					implementationAddress = `0x${storageValue.slice(26)}`;
					console.log(
						chalk.yellow(
							`  Detected proxy contract using storage slot with implementation at: ${implementationAddress}`,
						),
					);
				}
			} catch (storageError) {
				// Silent fail for storage slot check
			}
		}

		// Create the contract instances
		const proxyContract = new ethers.Contract(address, abi, provider);

		// If we found an implementation address, create a contract for it too
		const implementationContract = implementationAddress
			? new ethers.Contract(implementationAddress, abi, provider)
			: null;

		// First try with the implementation contract if available
		if (implementationContract) {
			try {
				await checkFn(implementationContract, name);
				return; // If successful, we're done
			} catch (implError) {
				console.log(
					chalk.yellow(
						"  Failed to check implementation contract, falling back to proxy contract",
					),
				);
				// Fall back to the proxy contract if implementation check fails
			}
		}

		// If no implementation contract or implementation check failed, try the proxy
		await checkFn(proxyContract, name);
	} catch (error) {
		console.error(chalk.red(`Error verifying ${name}:`), error);
	}
}

async function checkPausedFlags(
	contract: ethers.Contract,
	name: string,
): Promise<void> {
	let paused: bigint | null = null;
	let methodUsed = "";

	// Try the methods in this order: pausedFlags(), paused(), pauseWithdraw()

	// 1. First try pausedFlags() - most proxy implementations use this
	try {
		paused = await contract.pausedFlags();
		methodUsed = "pausedFlags()";
	} catch (e) {
		// Silent fail - we'll try other methods
	}

	// 2. If pausedFlags() failed, try paused()
	if (paused === null) {
		try {
			paused = await contract.paused();
			methodUsed = "paused()";
		} catch (e) {
			// Silent fail
		}
	}

	// If we got a paused value, display it
	if (paused !== null) {
		console.log(
			`  - Paused flags (from ${methodUsed}): ${paused} (${explainPauseFlags(BigInt(paused))})`,
		);

		const pausedBigInt = BigInt(paused);
		const isPauseDeposit = (pausedBigInt & 1n) === 1n;
		const isPauseWithdraw = (pausedBigInt & 2n) === 2n;

		// Different contracts use slightly different naming
		if (name === "ERC20 Locker") {
			console.log(
				`  - lockToken (deposits) paused: ${formatPauseStatus(isPauseDeposit)}`,
			);
			console.log(
				`  - unlockToken (withdrawals) paused: ${formatPauseStatus(isPauseWithdraw)}`,
			);
		} else if (name === "ETH Custodian Proxy" || name === "ETH Custodian") {
			console.log(
				`  - depositToNear/depositToEVM paused: ${formatPauseStatus(isPauseDeposit)}`,
			);
			console.log(`  - withdraw paused: ${formatPauseStatus(isPauseWithdraw)}`);
		} else if (name === "OmniBridge") {
			console.log(`  - deposits paused: ${formatPauseStatus(isPauseDeposit)}`);
			console.log(
				`  - withdrawals paused: ${formatPauseStatus(isPauseWithdraw)}`,
			);
		} else if (name === "eNEAR") {
			console.log(`  - deposits paused: ${formatPauseStatus(isPauseDeposit)}`);
			console.log(
				`  - withdrawals paused: ${formatPauseStatus(isPauseWithdraw)}`,
			);
		} else {
			console.log(`  - feature 1 paused: ${formatPauseStatus(isPauseDeposit)}`);
			console.log(
				`  - feature 2 paused: ${formatPauseStatus(isPauseWithdraw)}`,
			);
		}
		return;
	}

	// 3. Try pauseWithdraw for Bridge Token Factory
	if (name === "Bridge Token Factory") {
		try {
			const pauseWithdraw = await contract.pauseWithdraw();
			console.log(`  - pauseWithdraw: ${formatPauseStatus(pauseWithdraw)}`);
			return;
		} catch (e) {
			// Silent fail
		}
	}

	// 4. Try a manual check with a low-level call for proxy contracts
	try {
		// Sometimes proxy contracts need a more direct approach
		// This tries to call the pausedFlags function using a low-level call
		const callData = ethers.id("pausedFlags()").slice(0, 10); // get function selector
		const rawResult = await contract.provider.call({
			to: contract.target,
			data: callData,
		});

		if (rawResult && rawResult !== "0x") {
			// Decode the result - it should be a uint256
			const decodedResult = ethers.toNumber(rawResult);
			console.log(
				`  - Paused flags (from direct call): ${decodedResult} (${explainPauseFlags(BigInt(decodedResult))})`,
			);

			const pausedBigInt = BigInt(decodedResult);
			const isPauseDeposit = (pausedBigInt & 1n) === 1n;
			const isPauseWithdraw = (pausedBigInt & 2n) === 2n;

			console.log(`  - Deposits paused: ${formatPauseStatus(isPauseDeposit)}`);
			console.log(
				`  - Withdrawals paused: ${formatPauseStatus(isPauseWithdraw)}`,
			);
			return;
		}
	} catch (e) {
		// Silent fail for this low-level attempt
	}

	// If all methods failed, show a warning
	console.log(
		chalk.yellow("  - No pause status information found. All methods failed."),
	);
	console.log(
		chalk.yellow(
			"  - For proxy contracts, you may need to check on Etherscan directly.",
		),
	);
}

// New function to check Aurora Engine precompiles
async function checkAuroraPrecompiles(config: NearConfig): Promise<void> {
	try {
		// Initialize NEAR connection
		const keyStore = new keyStores.InMemoryKeyStore();
		const nearConnection = await connect({
			networkId: config.networkId,
			keyStore,
			nodeUrl: config.nodeUrl,
			headers: {},
		});

		// For view-only operations, we can use any dummy account
		const account = await nearConnection.account("dummy.near");
		const auroraContractId = config.contracts.aurora;

		if (!auroraContractId) {
			console.log(
				chalk.yellow("Aurora Engine contract ID not provided, skipping..."),
			);
			return;
		}

		console.log(
			chalk.cyan(`\nChecking Aurora Engine Precompiles (${auroraContractId}):`),
		);

		// Check paused precompiles flags
		try {
			const pausedPrecompiles = await account.viewFunction({
				contractId: auroraContractId,
				methodName: "paused_precompiles",
				args: {},
				parse: (result) => {
					return Buffer.from(result).readUInt32LE();
				},
			});

			console.log(`  - Paused precompiles flags: ${pausedPrecompiles}`);

			// Interpret flags based on PrecompileFlags in the Rust code
			const flags = pausedPrecompiles;

			// EXIT_TO_NEAR = 0b01 (1 in decimal)
			// EXIT_TO_ETHEREUM = 0b10 (2 in decimal)
			const exitToNearPaused = (flags & 1) === 1;
			const exitToEthereumPaused = (flags & 2) === 2;

			console.log(
				`  - EXIT_TO_NEAR precompile (bit 0) paused: ${formatPauseStatus(exitToNearPaused)}`,
			);
			console.log(
				`  - EXIT_TO_ETHEREUM precompile (bit 1) paused: ${formatPauseStatus(exitToEthereumPaused)}`,
			);

			// If both are paused, we should have flags = 3
			if (flags === 3) {
				console.log(
					chalk.green(
						"  ✓ Both EXIT_TO_NEAR and EXIT_TO_ETHEREUM precompiles are paused",
					),
				);
			} else if (flags === 0) {
				console.log(chalk.red("  ✗ No precompiles are paused"));
			} else {
				console.log(chalk.yellow("  ⚠ Only some precompiles are paused"));
			}
		} catch (e) {
			console.log(chalk.red(`  - Error checking paused_precompiles: ${e}`));
		}

		// Check FT transfer methods in ETH connector
		try {
			// This checks the pause flags for the internal_ft_methods in the ETH connector
			const etherConnectorFlags = await account.viewFunction({
				contractId: auroraContractId,
				methodName: "get_paused_flags",
				parse: (result) => {
					return Buffer.from(result).readUint8();
				},
			});

			console.log(`\n  - ETH Connector paused flags: ${etherConnectorFlags}`);

			// Based on the provided flags:
			// UNPAUSE_ALL = 0
			// PAUSE_DEPOSIT = 1 << 0 (1)
			// PAUSE_WITHDRAW = 1 << 1 (2)
			// PAUSE_FT = 1 << 2 (4)

			const flags = etherConnectorFlags;
			const depositPaused = (flags & 1) === 1;
			const withdrawPaused = (flags & 2) === 2;
			const ftPaused = (flags & 4) === 4;

			console.log(
				`  - Deposit methods paused: ${formatPauseStatus(depositPaused)}`,
			);
			console.log(
				`  - Withdraw methods paused: ${formatPauseStatus(withdrawPaused)}`,
			);
			console.log(
				`  - FT transfer methods paused: ${formatPauseStatus(ftPaused)}`,
			);

			// If all flags are set (255), all methods should be paused
			if (flags === 255) {
				console.log(chalk.green("  ✓ All ETH connector methods are paused"));
			} else if (flags === 0) {
				console.log(chalk.red("  ✗ No ETH connector methods are paused"));
			} else {
				console.log(
					chalk.yellow("  ⚠ Only some ETH connector methods are paused"),
				);

				// For more specific information
				if (ftPaused) {
					console.log(
						chalk.green("  ✓ FT transfer methods are paused (bit 2 set)"),
					);
				} else {
					console.log(
						chalk.red("  ✗ FT transfer methods are NOT paused (bit 2 not set)"),
					);
				}
			}
		} catch (e) {
			console.log(chalk.red(`  - Error checking ETH connector flags: ${e}`));
		}
	} catch (error) {
		console.error(
			chalk.red("Error verifying Aurora Engine precompiles:"),
			error,
		);
	}
}

async function verifyNearContracts(config: NearConfig): Promise<void> {
	try {
		// Initialize NEAR connection
		const keyStore = new keyStores.InMemoryKeyStore();
		const nearConnection = await connect({
			networkId: config.networkId,
			keyStore,
			nodeUrl: config.nodeUrl,
			headers: {},
		});

		// For view-only operations, we can use any dummy account
		const account = await nearConnection.account("dummy.near");

		// Check NEAR Token Locker
		await checkNearContract(
			account,
			"Token Locker",
			config.contracts.tokenLocker,
			[
				{ feature: "ft_on_transfer", description: "NEAR→ETH transfers" },
				{ feature: "withdraw", description: "ETH→NEAR transfers" },
			],
		);

		// Check eNEAR on NEAR
		await checkNearContract(
			account,
			"eNEAR Account",
			config.contracts.eNearAccount,
			[
				{ feature: "migrate_to_ethereum", description: "NEAR→ETH transfers" },
				{
					feature: "finalise_eth_to_near_transfer",
					description: "ETH→NEAR transfers",
				},
			],
		);

		// Check Bridge Token Factory on NEAR
		await checkNearContract(
			account,
			"Bridge Token Factory",
			config.contracts.bridgeTokenFactoryNear,
			[
				{
					feature: "deposit",
					description: "ETH→NEAR transfers",
				},
				{
					feature: "deploy_bridge_token",
					description: "Deploy bridge token",
				},
				{
					feature: "update_metadata",
					description: "Update metadata",
				},
			],
		);

		// Check OmniBridge on NEAR
		await checkNearContract(
			account,
			"OmniBridge NEAR",
			config.contracts.omniBridgeNear,
			[
				{
					feature: "ft_on_transfer",
					description: "NEAR→ETH transfers",
				},
				{
					feature: "fin_transfer",
					description: "ETH→NEAR transfers",
				},
				{
					feature: "log_metadata",
					description: "Log metadata",
				},
			],
		);
	} catch (error) {
		console.error(chalk.red("Error verifying NEAR contracts:"), error);
	}
}

async function checkNearContract(
	account: Account,
	name: string,
	contractId: string,
	features: Array<{ feature: string; description: string }>,
): Promise<void> {
	if (!contractId) {
		console.log(chalk.yellow(`${name} ID not provided, skipping...`));
		return;
	}

	console.log(chalk.cyan(`\nChecking ${name} (${contractId}):`));

	for (const feature of features) {
		try {
			const isPaused = await account.viewFunction({
				contractId,
				methodName: "pa_is_paused",
				args: { key: feature.feature },
			});

			console.log(
				`  - ${feature.feature} (${feature.description}) paused: ${formatPauseStatus(isPaused)}`,
			);
		} catch (e) {
			console.log(
				chalk.yellow(
					`  - pa_is_paused function not found or failed for ${feature.feature}`,
				),
			);
		}
	}
}

// Helper function to explain pause flag values
function explainPauseFlags(flags: bigint): string {
	const explanations: string[] = [];
	if ((flags & 1n) === 1n) explanations.push("Deposits/Locks paused (1)");
	if ((flags & 2n) === 2n) explanations.push("Withdrawals/Unlocks paused (2)");
	if ((flags & 4n) === 4n) explanations.push("Flag 4 paused");
	if ((flags & 8n) === 8n) explanations.push("Flag 8 paused");
	if ((flags & 16n) === 16n) explanations.push("Flag 16 paused");
	return explanations.join(", ") || "No features paused";
}

// Helper function to format pause status with colors
function formatPauseStatus(isPaused: boolean): string {
	return isPaused ? chalk.green("✓ PAUSED") : chalk.red("✗ NOT PAUSED");
}

// Execute the verification
verifyPausedMethods()
	.then(() => console.log(chalk.green.bold("\nVerification complete")))
	.catch((error) =>
		console.error(chalk.red.bold("\nVerification failed:"), error),
	);
