import { ethers } from 'ethers';
import type { TokenMetadata } from './types';
import { VerificationError } from './types';

const ERC20_ABI = [
    'function name() view returns (string)',
    'function symbol() view returns (string)',
    'function decimals() view returns (uint8)',
    'function balanceOf(address) view returns (uint256)'
];

const DEPLOY_TOKEN_EVENT_SIGNATURE = 'DeployToken(address,string,string,string,uint8,uint8)';

function getInfuraProvider() {
    return new ethers.JsonRpcProvider(`${process.env.ETH_RPC_URL}/${process.env.INFURA_API_KEY}`);
}

export async function verifyEvmTransaction(txHash: string): Promise<void> {
    const provider = getInfuraProvider();
    const receipt = await provider.getTransactionReceipt(txHash);

    if (!receipt) {
        throw new VerificationError(`Transaction ${txHash} not found`);
    }

    if (receipt.status === 0) {
        throw new VerificationError(`Transaction ${txHash} failed`);
    }
}

export async function getEvmTokenAddressFromTx(txHash: string): Promise<string> {
    const provider = getInfuraProvider();
    const receipt = await provider.getTransactionReceipt(txHash);
    if (!receipt) {
        throw new VerificationError(`Transaction ${txHash} not found`);
    }
    const tokenCreatedLog = receipt.logs.find(log => log.topics[0] === ethers.id(DEPLOY_TOKEN_EVENT_SIGNATURE));
    if (!tokenCreatedLog) {
        throw new VerificationError('TokenCreated event not found in transaction logs');
    }
    const evmTokenAddress = ethers.dataSlice(tokenCreatedLog.topics[1], 12);
    return evmTokenAddress;
}

export async function getEvmTokenMetadata(tokenAddress: string): Promise<TokenMetadata> {
    const provider = getInfuraProvider();
    const formattedAddress = ethers.getAddress(tokenAddress);
    const contract = new ethers.Contract(formattedAddress, ERC20_ABI, provider);

    const [name, symbol, decimals] = await Promise.all([
        contract.name(),
        contract.symbol(),
        contract.decimals()
    ]);

    return {
        name,
        symbol,
        decimals
    };
}

export async function verifyEvmTokenBalance(tokenAddress: string, accountAddress: string, expectedAmount: string): Promise<void> {
    const provider = getInfuraProvider();
    const formattedTokenAddress = ethers.getAddress(tokenAddress);
    const formattedAccountAddress = ethers.getAddress(accountAddress);
    const contract = new ethers.Contract(formattedTokenAddress, ERC20_ABI, provider);

    const balance = await contract.balanceOf(formattedAccountAddress);
    const actualBalance = balance.toString();

    if (actualBalance !== expectedAmount) {
        throw new VerificationError(
            `Token balance mismatch: expected ${expectedAmount}, but got ${actualBalance}`
        );
    }
} 

export async function getEvmLog(txHash: string, event_signature: string): Promise<ethers.Log> {
    const provider = getInfuraProvider();
    const receipt = await provider.getTransactionReceipt(txHash);
    if (!receipt) {
        throw new VerificationError(`Transaction ${txHash} not found`);
    }
    const logEntry = receipt.logs.find(l => l.topics.includes(ethers.id(event_signature)));
    if (!logEntry) {
        throw new VerificationError(`Event ${event_signature} not found in transaction ${txHash}`);
    }
    return logEntry;
}

export function addressToPaddedHex(address: string): string {
    const cleanAddress = address.startsWith('0x') ? address.slice(2) : address;
    
    if (cleanAddress.length !== 40) {
        throw new Error('Invalid address length. Expected 40 hex characters.');
    }
    
    const paddedAddress = cleanAddress.toLowerCase().padStart(64, '0');
    
    return '0x' + paddedAddress;
}
