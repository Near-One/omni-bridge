import { connect, keyStores, Contract } from "near-api-js";
import { ethers, JsonRpcProvider, parseUnits, Interface } from 'ethers'
import dotenv from 'dotenv'
import SafeApiKit from '@safe-global/api-kit'
import Safe from '@safe-global/protocol-kit'
import {
  MetaTransactionData,
  OperationType
} from '@safe-global/types-kit'
import Web3 from 'web3';

const erc20BalanceOfAbi = [
  {
    "constant": true,
    "inputs": [{ "name": "_owner", "type": "address" }],
    "name": "balanceOf",
    "outputs": [{ "name": "balance", "type": "uint256" }],
    "type": "function"
  }
];

async function initNear() {
  const keyStore = new keyStores.InMemoryKeyStore();
  const near_config = {
    networkId: process.env.NETWORK_NEAR!,
    keyStore,
    nodeUrl: `https://rpc.${process.env.NETWORK_NEAR!}.near.org`,
    walletUrl: `https://wallet.${process.env.NETWORK_NEAR!}.near.org`,
    helperUrl: `https://helper.${process.env.NETWORK_NEAR!}.near.org`
  };

  return await connect(near_config);
}

async function getTotalSupply(token_id: string) {
    console.log(token_id)

    const near = await initNear();
    const account = await near.account("script_account.near");

    const contract = new Contract(account, token_id, {
      viewMethods: ["ft_total_supply"],
      changeMethods: [],
      useLocalViewExecution: true
    });

    const result = await account.viewFunction({
        contractId:  token_id,
        methodName: "ft_total_supply",
        args: {}});

    return result
}

async function extractTokensList() {
    const near = await initNear();
    const account = await near.account("script_account.near");

    const contract = new Contract(account, process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID!, {
      viewMethods: ["get_tokens"],
      changeMethods: [],
      useLocalViewExecution: true
    });

    const result = await account.viewFunction({
        contractId: process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID!,
        methodName: "get_tokens",
        args: {}});

    return result
}

(async () => {
    dotenv.config()

    const RPC_URL=`https://` + process.env.NETWORK_ETH! + `.infura.io/v3/` + process.env.INFURA_API_KEY!
    const web3 = new Web3(RPC_URL);

    const provider = new JsonRpcProvider(RPC_URL)

    const owner1Signer = new ethers.Wallet(process.env.PRIVATE_KEY!, provider)
    const apiKit = new SafeApiKit({
        chainId: process.env.CHAIN_ID!
    })

    const protocolKitOwner1 = await Safe.init({
        provider: RPC_URL,
        signer: process.env.PRIVATE_KEY!,
        safeAddress: process.env.SAFE_ADDRESS!
    })

    const destination = process.env.ERC20_LOCKER!;

    const erc20Abi = ["function adminTransfer(address,address,uint256)", "function balanceOf(address) view returns (uint256)"];
    const erc20Interface = new Interface(erc20Abi);

    const tokens_list = await extractTokensList();
    console.log(tokens_list)
    const txs = [];
    for (let i = 0; i < tokens_list.length; i++) {
        const contract = new web3.eth.Contract(erc20BalanceOfAbi, tokens_list[i]);
        const balance = await contract.methods.balanceOf(destination).call();
        const total_supply = await getTotalSupply(tokens_list[i] + "." + process.env.BRIDGE_TOKEN_FACTORY_ACCOUNT_ID!);
        console.log("ERC20 token: ", tokens_list[i], ", balance=", balance, "total_supply=", total_supply);
        const data = erc20Interface.encodeFunctionData("adminTransfer", [tokens_list[i], process.env.OMNI_LOCKER!, total_supply]);

        const safeTransactionData: MetaTransactionData = {
            to: destination,
            value: '0',
            data: data,
            operation: OperationType.Call
        };

        txs.push(safeTransactionData);
    }

    const safeTransaction = await protocolKitOwner1.createTransaction({
        transactions: txs
    })

    const safeTxHash = await protocolKitOwner1.getTransactionHash(safeTransaction)
    const signature = await protocolKitOwner1.signHash(safeTxHash)

    // Propose transaction to the service
    await apiKit.proposeTransaction({
        safeAddress: process.env.SAFE_ADDRESS!,
        safeTransactionData: safeTransaction.data,
        safeTxHash,
        senderAddress: owner1Signer.address,
        senderSignature: signature.data
    })

    console.log(safeTxHash);
})()
