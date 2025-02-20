import { connect, keyStores, Contract } from "near-api-js";
import { ethers, JsonRpcProvider, Interface } from 'ethers'
import dotenv from 'dotenv'
import SafeApiKit from '@safe-global/api-kit'
import Safe from '@safe-global/protocol-kit'
import type {
  MetaTransactionData
} from '@safe-global/types-kit'
import { OperationType } from '@safe-global/types-kit'
import Web3 from 'web3';

dotenv.config();

const abiEthCustodian = [
    "function adminSendEth(address payable destination, uint256 amount)"
];

async function initNear() {
  const keyStore = new keyStores.InMemoryKeyStore();
  const near_config = {
    networkId: process.env.NETWORK_NEAR || '',
    keyStore,
    nodeUrl: `https://rpc.${process.env.NETWORK_NEAR}.near.org`,
    walletUrl: `https://wallet.${process.env.NETWORK_NEAR}.near.org`,
    helperUrl: `https://helper.${process.env.NETWORK_NEAR}.near.org`
  };

  return await connect(near_config);
}

async function getTotalSupply() {
    const near = await initNear();
    const account = await near.account("script_account.near");

    const result = await account.viewFunction({
        contractId:  process.env.ETH_ACCOUNT_ID || '',
        methodName: "ft_total_supply",
        args: {}});

    return result
}

(async () => {
    dotenv.config()

    const RPC_URL = `https://${process.env.NETWORK_ETH}.infura.io/v3/${process.env.INFURA_API_KEY}`;
    const web3 = new Web3(RPC_URL);

    const provider = new JsonRpcProvider(RPC_URL)
    const owner1Signer = new ethers.Wallet(process.env.PRIVATE_KEY || '', provider)
    const apiKit = new SafeApiKit({
        chainId: BigInt(process.env.CHAIN_ID || '0')
    })

    const protocolKitOwner1 = await Safe.init({
        provider: RPC_URL,
        signer: process.env.PRIVATE_KEY || '',
        safeAddress: process.env.SAFE_ADDRESS || ''
    })

    const balance = await provider.getBalance(process.env.ETH_CUSTODIAN || '');
    const total_supply = await getTotalSupply();
    console.log("Eth Custodian Balance:", balance, "; Total Supply Eth Account on Near: ", total_supply);

    const ethCustodianInterface = new ethers.Interface(abiEthCustodian);
    const data = ethCustodianInterface.encodeFunctionData("adminSendEth", [process.env.SAFE_ADDRESS || '', total_supply]);

    console.log("Encoded data for admin send eth: ", data);

    const destination = process.env.ETH_CUSTODIAN_PROXY || '';
    const ethCustodianProxyAbi = ["function callImpl(bytes)"];
    const ethCustodianProxyInterface = new Interface(ethCustodianProxyAbi);

    const txs = [];
    const callImplData = ethCustodianProxyInterface.encodeFunctionData("callImpl", [data]);

    const safeTransactionData: MetaTransactionData = {
        to: destination,
        value: '0',
        data: callImplData,
        operation: OperationType.Call
    };

    txs.push(safeTransactionData);

    const safeTransactionSendEthData: MetaTransactionData = {
        to: process.env.OMNI_LOCKER,
        value: total_supply,
        data: "0x"
    };

    txs.push(safeTransactionSendEthData);

    const safeTransaction = await protocolKitOwner1.createTransaction({
       transactions: txs
    });

    const safeTxHash = await protocolKitOwner1.getTransactionHash(safeTransaction)
    const signature = await protocolKitOwner1.signHash(safeTxHash)

    // Propose transaction to the service
    await apiKit.proposeTransaction({
       safeAddress: process.env.SAFE_ADDRESS || '',
       safeTransactionData: safeTransaction.data,
       safeTxHash,
       senderAddress: owner1Signer.address,
       senderSignature: signature.data
    })

    console.log(safeTxHash);
})()
