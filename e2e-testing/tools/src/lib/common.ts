import * as fs from 'node:fs';
import * as path from 'node:path';
import { verifyNearReceipt } from './near';
import { verifyEvmTransaction } from './evm';
import type { TransactionInfo } from './types';

export async function loadTransactions(dir: string): Promise<TransactionInfo[]> {
    const files = fs.readdirSync(dir).filter(file => file.endsWith('.json'));
    const transactions: TransactionInfo[] = [];

    for (const file of files) {
        const content = JSON.parse(fs.readFileSync(path.join(dir, file), 'utf-8'));
        if (content.tx_hash) {
            const network = content.tx_hash.startsWith('0x') ? 'evm' : 'near';
            transactions.push({ hash: content.tx_hash, network });
        }
    }

    return transactions;
}

export async function verifyTransactions(transactions: TransactionInfo[]): Promise<void> {
    for (const tx of transactions) {
        if (tx.network === 'near') {
            await verifyNearReceipt(tx.hash);
        } else {
            await verifyEvmTransaction(tx.hash);
        }
    }
}
