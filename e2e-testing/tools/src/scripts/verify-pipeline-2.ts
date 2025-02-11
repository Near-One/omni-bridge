import 'dotenv/config';
import { Command } from 'commander';
import { verifyEvmTokenBalance } from '../lib/evm';
import { loadTransactions, verifyTransactions } from '../lib/common';
import { VerificationError } from '../lib/types';

async function main() {
    const program = new Command();

    program
        .requiredOption('-d, --tx-dir <dir>', 'Directory containing transaction receipts')
        .requiredOption('-t, --token <address>', 'ERC20 token address')
        .requiredOption('-a, --account <address>', 'Account address to check balance for')
        .requiredOption('-b, --balance <amount>', 'Expected token balance')
        .parse(process.argv);

    const options = program.opts();

    try {
        const transactions = await loadTransactions(options.txDir);
        await verifyTransactions(transactions);
        console.log('All pipeline transactions verified successfully!');

        await verifyEvmTokenBalance(options.token, options.account, options.balance);
        console.log('Token balance verified successfully!');

        console.log('All verifications passed successfully!');
    } catch (error) {
        if (error instanceof VerificationError) {
            console.error('Verification failed:', error.message);
        } else {
            console.error('Unexpected error:', error);
        }
        process.exit(1);
    }
}

main(); 