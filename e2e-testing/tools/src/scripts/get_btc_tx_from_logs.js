#!/usr/bin/env node
const fs = require("fs");
const axios = require("axios");

const BRIDGE_BASE =
  "https://testnet.api.bridge.nearone.org/api/v2/transfers/transfer";
const NEAR_RPC = "https://rpc.testnet.near.org";

async function fetchBridgeTransfer(txHashNear) {
  const url = `${BRIDGE_BASE}?transaction_hash=${txHashNear}`;
  const { data } = await axios.get(url);
  if (!Array.isArray(data) || data.length === 0) {
    throw new Error("Bridge API: empty response");
  }
  return data[0];
}

async function fetchNearTx(txHash, signerId) {
  const body = {
    jsonrpc: "2.0",
    id: "dontcare",
    method: "EXPERIMENTAL_tx_status",
    params: [txHash, signerId],
  };
  const { data } = await axios.post(NEAR_RPC, body, {
    headers: { "Content-Type": "application/json" },
  });
  if (data.error) {
    throw new Error(
      `NEAR RPC error: ${data.error.code} ${data.error.message}`,
    );
  }
  return data.result;
}

function extractBtcPendingSignId(txResult) {
  const receipts = txResult.receipts || [];
  for (const r of receipts) {
    const actions = r?.receipt?.Action?.actions || [];
    for (const action of actions) {
      if (!action.FunctionCall) continue;
      const fc = action.FunctionCall;
      const decoded = Buffer.from(fc.args, "base64").toString("utf-8");
      let parsed;
      try {
        parsed = JSON.parse(decoded);
      } catch (_) {
        continue;
      }
      if (parsed.btc_pending_sign_id) {
        return parsed.btc_pending_sign_id;
      }
    }
  }
  return null;
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 2) {
    console.error(
      "Usage: node get_btc_pending_sign_id.js <near_tx_hash> <output_json>",
    );
    process.exit(1);
  }

  const [initTxHash, outputPath] = args;

  console.log(
    `[1/3] Fetching bridge transfer by initial tx: ${initTxHash} ...`,
  );
  const transfer = await fetchBridgeTransfer(initTxHash);

  const signedTxHash =
    transfer?.signed?.NearReceipt?.transaction_hash ||
    transfer?.signed?.Near?.transaction_hash;
  if (!signedTxHash) {
    throw new Error("No signed transaction hash in bridge transfer");
  }

  const signerId = transfer?.utxo_transfer?.sender;
  if (!signerId) {
    throw new Error("No sender account_id in transfer.utxo_transfer.sender");
  }

  console.log(
    `[2/3] Fetching NEAR tx ${signedTxHash} (signer: ${signerId}) ...`,
  );
  const nearTx = await fetchNearTx(signedTxHash, signerId);

  console.log("[3/3] Extracting btc_pending_sign_id ...");
  const btcPendingSignId = extractBtcPendingSignId(nearTx);

  if (!btcPendingSignId) {
    console.error("btc_pending_sign_id not found in transaction actions");
  }

  const result = { btc_pending_sign_id: btcPendingSignId };
  fs.writeFileSync(outputPath, JSON.stringify(result, null, 2));
  console.log(`✅ Written to ${outputPath}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
