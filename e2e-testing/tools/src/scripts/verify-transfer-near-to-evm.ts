import 'dotenv/config';
import { Command } from 'commander';
import { getLockerTokenAddress } from '../lib/near';
import { getEvmLog, addressToPaddedHex } from '../lib/evm';
import { loadTransactions, verifyTransactions } from '../lib/common';
import { VerificationError } from '../lib/types';

async function main() {
    const program = new Command();

    program
        .requiredOption('-d, --tx-dir <dir>', 'Directory containing transaction receipts')
        .requiredOption('-r, --receiver <receiverAddress>', 'Receiver address on the EVM chain')
        .requiredOption('-n, --near-token <address>', 'NEAR token address')
        .requiredOption('-c, --chain-kind <chain-kind>', 'Chain kind')
        .requiredOption('-l, --near-locker <address>', 'NEAR locker address')
        .parse(process.argv);

    const options = program.opts();

    try {
        const transactions = await loadTransactions(options.txDir);

        await verifyTransactions(transactions);
        console.log('Transactions verified successfully!');

        const log = await getEvmLog(transactions[transactions.length - 1].hash, 'Transfer(address,address,uint256)');

        const evmTokenAddress = await getLockerTokenAddress(options.nearLocker, options.nearToken, options.chainKind);

        if (`eth:${log.address.toLowerCase()}` !== evmTokenAddress) {
            throw new VerificationError(`Token address in log (${log.address}) does not match expected token address (${evmTokenAddress})`);
        }
        if (log.topics[1] !== '0x0000000000000000000000000000000000000000000000000000000000000000') {
            throw new VerificationError('Tokens were not minted, expected zero address in log topic 1');
        }
        if (log.topics[2] !== addressToPaddedHex(options.receiver)) {
            throw new VerificationError(`Receiver address in log (${log.topics[2]}) does not match expected receiver address (${options.receiver})`);
        }
        console.log('Tokens transferred successfully!');

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