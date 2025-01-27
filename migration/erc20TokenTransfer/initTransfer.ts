import { ethers, JsonRpcProvider, parseUnits, Interface } from 'ethers'
import EthersAdapter from '@safe-global/protocol-kit'
import dotenv from 'dotenv'
import SafeApiKit from '@safe-global/api-kit'
import Safe from '@safe-global/protocol-kit'
import SafeFactory from '@safe-global/protocol-kit'
import { SafeAccountConfig } from '@safe-global/protocol-kit'
import { SafeTransactionDataPartial } from '@safe-global/safe-core-sdk-types'
import {
  MetaTransactionData,
  OperationType
} from '@safe-global/types-kit'
import Web3 from 'web3';


import { tokens_list } from './tokensList'
import { config } from './config'

const erc20BalanceOfAbi = [
  {
    "constant": true,
    "inputs": [{ "name": "_owner", "type": "address" }],
    "name": "balanceOf",
    "outputs": [{ "name": "balance", "type": "uint256" }],
    "type": "function"
  }
];

(async () => {
    dotenv.config()

    const RPC_URL=`https://` + config.network + `.infura.io/v3/` + process.env.INFURA_API_KEY!
    const web3 = new Web3(RPC_URL);

    const provider = new JsonRpcProvider(RPC_URL)

    const owner1Signer = new ethers.Wallet(process.env.PRIVATE_KEY!, provider)
    const apiKit = new SafeApiKit({
        chainId: config.chainId
    })
    
    const protocolKitOwner1 = await Safe.init({
        provider: RPC_URL,
        signer: process.env.PRIVATE_KEY!,
        safeAddress: config.safe_address
    })

    const destination = config.erc20_locker;
    
    const erc20Abi = ["function adminTransfer(address,address,uint256)", "function balanceOf(address) view returns (uint256)"];
    const erc20Interface = new Interface(erc20Abi);
    
    const txs = [];
    for (let i = 0; i < tokens_list.length; i++) {
        const contract = new web3.eth.Contract(erc20BalanceOfAbi, tokens_list[i]);    
        const balance = await contract.methods.balanceOf(destination).call();
        console.log("ERC20 token: ", tokens_list[i], ", balance=", balance);
        const data = erc20Interface.encodeFunctionData("adminTransfer", [tokens_list[i], config.omni_locker, balance]);

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
        safeAddress: config.safe_address,
        safeTransactionData: safeTransaction.data,
        safeTxHash,
        senderAddress: owner1Signer.address,
        senderSignature: signature.data
    })
    
    console.log(safeTxHash);
})()
