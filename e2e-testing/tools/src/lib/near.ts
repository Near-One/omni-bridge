import { connect, providers } from 'near-api-js';
import type { TokenMetadata } from './types';
import { VerificationError } from './types';

class NearClient {
    private provider: providers.JsonRpcProvider;

    constructor() {
        this.provider = new providers.JsonRpcProvider({
            url: process.env.NEAR_NODE_URL || 'https://rpc.testnet.near.org'
        });
    }

    async getNearLog(txHash: string, receiptIdx: number, logIdx: number): Promise<string> {
        const txStatus = await this.provider.txStatus(txHash, 'system');

        // Check main transaction status
        const status = txStatus.status as { SuccessValue?: string; Failure?: unknown };
        if (status.Failure) {
            throw new VerificationError(`NEAR transaction ${txHash} failed: ${JSON.stringify(status.Failure)}`);
        }

        return txStatus.receipts_outcome[receiptIdx].outcome.logs[logIdx];
    }

    async verifyNearReceipt(txHash: string): Promise<void> {
        const txStatus = await this.provider.txStatus(txHash, 'system');

        // Check main transaction status
        const status = txStatus.status as { SuccessValue?: string; Failure?: unknown };
        if (status.Failure) {
            throw new VerificationError(`NEAR transaction ${txHash} failed: ${JSON.stringify(status.Failure)}`);
        }

        // Check all receipt statuses
        for (const receipt of txStatus.receipts_outcome || []) {
            const receiptStatus = receipt.outcome.status as { SuccessValue?: string; Failure?: unknown };
            if (receiptStatus.Failure) {
                throw new VerificationError(
                    `Receipt ${receipt.id} of transaction ${txHash} failed: ${JSON.stringify(receiptStatus.Failure)}`
                );
            }
        }
    }

    async getTokenMetadata(tokenAddress: string): Promise<TokenMetadata> {
        const response = await this.provider.query({
            request_type: 'call_function',
            account_id: tokenAddress,
            method_name: 'ft_metadata',
            args_base64: Buffer.from('{}').toString('base64'),
            finality: 'final'
        });

        if ('result' in response) {
            const metadata = JSON.parse(Buffer.from(response.result as number[]).toString());
            return {
                name: metadata.name,
                symbol: metadata.symbol,
                decimals: metadata.decimals
            };
        }
        throw new VerificationError('Invalid response from ft_metadata call');
    }

    async getLockerTokenAddress(lockerAddress: string, nearToken: string, evmChainKind: string): Promise<string> {
        // TODO: Fix it with the correct call.
        const response = await this.provider.query({
            request_type: 'call_function',
            account_id: lockerAddress,
            method_name: 'get_token_address',
            args_base64: Buffer.from(JSON.stringify({
                chain_kind: evmChainKind,
                token: nearToken.toLowerCase()
            })).toString('base64'),
            finality: 'final'
        });

        if ('result' in response) {
            return JSON.parse(Buffer.from(response.result as number[]).toString());
        }
        throw new VerificationError('Invalid response from get_token_address call');
    }
}

const nearClient = new NearClient();

export const verifyNearReceipt = (txHash: string) => nearClient.verifyNearReceipt(txHash);
export const getNearTokenMetadata = (tokenAddress: string) => nearClient.getTokenMetadata(tokenAddress);
export const getLockerTokenAddress = (lockerAddress: string, nearToken: string, evmChainKind: string) => nearClient.getLockerTokenAddress(lockerAddress, nearToken, evmChainKind); 
export const getNearLog = (txHash: string, receiptIdx: number, logIdx: number) => nearClient.getNearLog(txHash, receiptIdx, logIdx);