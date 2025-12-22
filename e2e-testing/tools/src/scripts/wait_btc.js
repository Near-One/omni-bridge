#!/usr/bin/env node
const fs = require("fs");
const axios = require("axios");

const REQUIRED_CONFIRMATIONS = 3;
const POLL_INTERVAL = 30_000; // 30 seconds

async function getTipHeight() {
  const res = await axios.get(`https://blockstream.info/testnet/api/blocks/tip/height`);
  return Number(res.data);
}

async function getConfirmations(txid) {
  const url = `https://blockstream.info/testnet/api/tx/${txid}`;
  const res = await axios.get(url);
  const data = res.data;
  const tipHeight = await getTipHeight();

  if (data && data.status) {
    if (data.status.confirmed) {
      return tipHeight - data.status.block_height + 1;
    } else {
      return 0;
    }
  }
  return 0;
}

async function waitForConfirmations(txid, outputPath) {
  console.log(
    `Waiting for BTC transaction ${txid} to reach ${REQUIRED_CONFIRMATIONS} confirmations...`,
  );

  while (true) {
    try {
      const confs = await getConfirmations(txid);
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
  if (args.length < 2) {
    console.error("Usage: node wait_btc_confirmations.js <txid> <output_json>");
    process.exit(1);
  }

  const [txid, outputPath] = args;

  if (!txid) {
    console.error("Error: txid is empty");
    process.exit(1);
  }

  await waitForConfirmations(txid, outputPath);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
