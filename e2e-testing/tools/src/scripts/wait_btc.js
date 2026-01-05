#!/usr/bin/env node
const fs = require("fs");
const axios = require("axios");

const REQUIRED_CONFIRMATIONS = 3;
const POLL_INTERVAL = 30_000; // 30 seconds

async function getTipHeight() {
  const res = await axios.get(`https://blockstream.info/testnet/api/blocks/tip/height`);
  return Number(res.data);
}

async function getZcashConfirmations(txid) {
  const res = await axios.post(
      "https://zcash-testnet.gateway.tatum.io",
      {
        jsonrpc: "2.0",
        id: "get-zec-tx",
        method: "getrawtransaction",
        params: [txid, 1],
      },
      {
        headers: {
          "Content-Type": "application/json",
          "x-api-key": process.env.TATUM_API_KEY,
        },
        timeout: 30_000,
      }
  );

  const tx = res.data?.result;
  if (!tx) return 0;

  return typeof tx.confirmations === "number" ? tx.confirmations : 0;
}

async function getConfirmations(chain, txid) {
  if (chain === "btc") {
    const url = `https://blockstream.info/testnet/api/tx/${txid}`;
    const res = await axios.get(url);
    const data = res.data;

    if (!data?.status?.confirmed) return 0;

    const tipHeight = await getTipHeight("btc");
    return tipHeight - data.status.block_height + 1;
  }

  if (chain === "zcash") {
    return await getZcashConfirmations(txid);
  }

  throw new Error(`Unsupported chain: ${chain}`);
}

async function waitForConfirmations(chain, txid, outputPath) {
  console.log(
    `Waiting for BTC transaction ${txid} to reach ${REQUIRED_CONFIRMATIONS} confirmations...`,
  );

  while (true) {
    try {
      const confs = await getConfirmations(chain, txid);
      console.log(
        `[${new Date().toISOString()}] confirmations = ${confs}`,
      );

      if (confs >= REQUIRED_CONFIRMATIONS) {
        console.log(
          `✅ Transaction ${txid} has ${confs} confirmations!`,
        );

        fs.writeFileSync(
          outputPath,
          JSON.stringify({ txid, confirmations: confs }, null, 2),
        );

        await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL));
        console.log(`Result saved to: ${outputPath}`);
        return;
      }
    } catch (err) {
      console.error(
        `[${new Date().toISOString()}] Error fetching tx: ${err.message}`,
      );
    }

    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL));
  }
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 3) {
    console.error(
      "Usage: node wait_confirmations.js <btc|zcash> <txid> <output_json>"
    );
    process.exit(1);
  }

  const [chain, txid, outputPath] = args;

  if (!["btc", "zcash"].includes(chain)) {
    console.error("Error: chain must be 'btc' or 'zcash'");
    process.exit(1);
  }

  if (!txid) {
    console.error("Error: txid is empty");
    process.exit(1);
  }

  await waitForConfirmations(chain, txid, outputPath);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
