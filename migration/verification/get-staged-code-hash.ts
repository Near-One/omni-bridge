import { connect, keyStores } from "near-api-js";
import dotenv from "dotenv";
import chalk from "chalk";
import { Buffer } from "buffer";

dotenv.config();

interface NearConfig {
	networkId: string;
	nodeUrl: string;
}

async function get_staged_code_hash(config: NearConfig): Promise<void> {
	const contractId = process.env.OMNI_BRIDGE_ACCOUNT_ID || ""

	if (!contractId) {
		console.log(chalk.yellow(`${contractId} ID not provided, skipping...`));
		return;
	}


	const keyStore = new keyStores.InMemoryKeyStore();
	const nearConnection = await connect({
		networkId: config.networkId,
		keyStore,
		nodeUrl: config.nodeUrl,
		headers: {},
	});
	const account = await nearConnection.account("dummy.near");

	try {
		const up_staged_code_hash = await account.viewFunction({
			contractId,
			methodName: "up_staged_code_hash",
			args: {},
		});
		const buffer = Buffer.from(up_staged_code_hash);
		const base64Encoded = buffer.toString("base64");

		console.log("Base64:", base64Encoded);
	} catch (e) {
		console.log(
			chalk.yellow(
				`up_staged_code_hash function not found or failed`,
			),
		);
	}
}

const nearConfig: NearConfig = {
	networkId: process.env.NETWORK_NEAR || "testnet",
	nodeUrl: `https://rpc.${process.env.NETWORK_NEAR || "testnet"}.near.org`,
};
get_staged_code_hash(nearConfig)
