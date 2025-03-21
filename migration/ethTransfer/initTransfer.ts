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
        contractId:  process.env.AURORA_ACCOUNT_ID || '',
        methodName: "ft_total_supply",
        args: {}});

    return result
}

(async () => {
    dotenv.config()

    const RPC_URL = `https://${process.env.NETWORK_ETH}.infura.io/v3/${process.env.INFURA_API_KEY}`;
    const web3 = new Web3(RPC_URL);

    const provider = new JsonRpcProvider(RPC_URL)
    const owner1Signer = new ethers.Wallet(process.env.EVM_PRIVATE_KEY || '', provider)
    const apiKit = new SafeApiKit({
        chainId: BigInt(process.env.CHAIN_ID || '0')
    })

    const protocolKitOwner1 = await Safe.init({
        provider: RPC_URL,
        signer: process.env.EVM_PRIVATE_KEY || '',
        safeAddress: process.env.MULTISIG_ADDRESS || ''
    })

    const balance = await provider.getBalance(process.env.ETH_CUSTODIAN || '');
    const total_supply = await getTotalSupply();
    const transfer_value = total_supply;
    console.log("Eth Custodian Balance:", balance, "; Total Supply Eth Account on Near: ", total_supply);

    const ethCustodianInterface = new ethers.Interface(abiEthCustodian);
    const data = ethCustodianInterface.encodeFunctionData("adminSendEth", [process.env.MULTISIG_ADDRESS || '', transfer_value]);

    console.log("Encoded data for admin send eth: ", data);

    const destination = process.env.ETH_CUSTODIAN || '';
    const nonce = await protocolKitOwner1.getNonce(); 
    const safeTransactionData: MetaTransactionData = {
        to: destination,
        value: '0',
        data: data,
        operation: OperationType.Call
    };

    const safeTransaction = await protocolKitOwner1.createTransaction({
       transactions: [safeTransactionData]
    });

    const safeTxHash = await protocolKitOwner1.getTransactionHash(safeTransaction)
    const signature = await protocolKitOwner1.signHash(safeTxHash)

    // Propose transaction to the service
    await apiKit.proposeTransaction({
       safeAddress: process.env.MULTISIG_ADDRESS || '',
       safeTransactionData: safeTransaction.data,
       safeTxHash,
       senderAddress: owner1Signer.address,
       senderSignature: signature.data
    })

    console.log(safeTxHash);

    const safeTransactionSendEthData: MetaTransactionData = {
        to: process.env.OMNI_BRIDGE_ETH,
        value: transfer_value,
        data: "0x",
	operation: OperationType.Call
    };

    const safeTransaction2 = await protocolKitOwner1.createTransaction({
       transactions: [safeTransactionSendEthData],
       options: {
           nonce: nonce + 1
       }
    });

    const safeTxHash2 = await protocolKitOwner1.getTransactionHash(safeTransaction2)
    const signature2 = await protocolKitOwner1.signHash(safeTxHash2)

    // Propose transaction to the service
    await apiKit.proposeTransaction({
       safeAddress: process.env.MULTISIG_ADDRESS || '',
       safeTransactionData: safeTransaction2.data,
       safeTxHash: safeTxHash2,
       senderAddress: owner1Signer.address,
       senderSignature: signature2.data
    })

    console.log(safeTxHash2);
})()
