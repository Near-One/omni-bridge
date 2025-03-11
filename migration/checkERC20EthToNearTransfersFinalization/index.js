import { connect, keyStores, Contract } from "near-api-js";
import * as borsh from 'borsh';
import dotenv from 'dotenv'
import { Header } from 'eth-object';
import { ethers } from "ethers";
import { Formatter } from "@ethersproject/providers";

dotenv.config();

const ABI = [
  "event Locked(address indexed token, address indexed sender, uint256 amount, string accountId)"
];

const provider = new ethers.JsonRpcProvider(`https://${process.env.NETWORK_ETH}.infura.io/v3/${process.env.INFURA_API_KEY}`);
const contract = new ethers.Contract(process.env.ERC20_LOCKER, ABI, provider);

const schema = { struct: {
      log_index: "u64",
      log_entry_data: { array: { type: 'u8' }},
      receipt_index: "u64",
      receipt_data: { array: { type: 'u8' }},
      header_data: { array: { type: 'u8' }},
      proof: {array: {type: { array: { type: 'u8' }}}} }};

async function initNear() {
  const keyStore = new keyStores.InMemoryKeyStore();
  const near_config = {
    networkId: process.env.NETWORK_NEAR,
    keyStore,
    nodeUrl: `https://rpc.${process.env.NETWORK_NEAR}.near.org`,
    walletUrl: `https://wallet.${process.env.NETWORK_NEAR}.near.org`,
    helperUrl: `https://helper.${process.env.NETWORK_NEAR}.near.org`
  };

  return await connect(near_config);
}

function bytesBorshStringify(input) {
    return Buffer.from(input);
}

async function isUsedProof(proof) {
    const near = await initNear();
    const account = await near.account("script_account.near");

    const contract = new Contract(account, process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID, {
      viewMethods: ["is_used_proof"],
      changeMethods: []
    });

    const result = await account.viewFunction({
        contractId: process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID,
        methodName: "is_used_proof",
        args: proof,
        stringify: bytesBorshStringify});

    return result
}


async function getLatestEvents() {
    const latestBlock = await provider.getBlockNumber();
    const fromBlock = latestBlock - 3000;

    const events = await contract.queryFilter("Locked", fromBlock, latestBlock);

    console.log(events.length, "transactions detected");
    let cnt_not_fin = 0;
    for (let i = 0; i < events.length; i++) {
      let event = events[i];
      const block = await provider.send(
        'eth_getBlockByNumber',
        [ethers.toBeHex(event.blockNumber), false]);
      const header_rlp = Header.fromRpc(block).serialize();

      const rpcObjFormatter = new Formatter();
      const receipt = rpcObjFormatter.receipt(await provider.send('eth_getTransactionReceipt', [event.transactionHash]));

      const logIndexInArray = receipt.logs.findIndex(
         l => l.logIndex == event.index
      );

      const proofLight = {
          "log_index": logIndexInArray,
          "log_entry_data": [],
          "receipt_index": event.transactionIndex,
          "receipt_data": [],
          "header_data": header_rlp,
          "proof": [],
      }

      const serializedProof = borsh.serialize(schema, proofLight);
      const res = await isUsedProof(serializedProof);

      if (res == false) {
         cnt_not_fin += 1;
         console.log("Transaction ", event.transactionHash, " is NOT finalize");
      } else {
         console.log("Transaction ", event.transactionHash, " is finalize");
      }
    }

    console.log();

    if (cnt_not_fin > 0) {
        console.log(cnt_not_fin, " transactions are NOT finalize, wait for finalization!");
    } else {
        console.log("All transactions are finalize! You can move to the next step!");
    }
}

getLatestEvents();
