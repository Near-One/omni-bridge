export interface TokenMetadata {
    name: string;
    symbol: string;
    decimals: number;
}

export interface TokenBridgeVerificationConfig {
    nearTokenAddress: string;
    evmTokenTxHash: string;
    nearLockerAddress: string;
    receiptsDir: string;
    chainKind: string;
}

export interface TransactionInfo {
    hash: string;
    network: 'near' | 'evm';
}

export class VerificationError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'VerificationError';
    }
} 