import 'dotenv/config';
import { Command } from 'commander';
import { getNearTokenMetadata, getLockerTokenAddress } from '../lib/near';
import { getEvmTokenMetadata, getEvmTokenAddressFromTx } from '../lib/evm';
import { loadTransactions, verifyTransactions } from '../lib/common';
import type { TokenBridgeVerificationConfig } from '../lib/types';
import { VerificationError } from '../lib/types';

async function verifyTokenMetadata(nearToken: string, evmToken: string): Promise<void> {
    const [nearMetadata, evmMetadata] = await Promise.all([
        getNearTokenMetadata(nearToken),
        getEvmTokenMetadata(evmToken)
    ]);

    if (nearMetadata.name !== evmMetadata.name) {
        throw new VerificationError(`Token names don't match: NEAR: ${nearMetadata.name} vs EVM: ${evmMetadata.name}`);
    }
    if (nearMetadata.symbol !== evmMetadata.symbol) {
        throw new VerificationError(`Token symbols don't match: NEAR: ${nearMetadata.symbol} vs EVM: ${evmMetadata.symbol}`);
    }

    // According to Bridge logic EVM token decimals should be 18 or less
    const expectedEvmDecimals = Math.min(18, Number(nearMetadata.decimals));
    if (Number(evmMetadata.decimals) !== expectedEvmDecimals) {
        throw new VerificationError(`Token decimals don't match: NEAR: ${nearMetadata.decimals} vs EVM: ${evmMetadata.decimals}`);
    }
}

async function verifyLockerTokenAddress(lockerAddress: string, nearToken: string, evmTokenAddress: string, evmChainKind: string): Promise<void> {
    const actualEvmToken = await getLockerTokenAddress(lockerAddress, nearToken, evmChainKind);
    const actualAddress = actualEvmToken.toLowerCase();
    const expectedAddress = `${evmChainKind.toLowerCase()}:${evmTokenAddress.toLowerCase()}`;
    if (actualAddress !== expectedAddress) {
        throw new VerificationError(
            `Locker's token address doesn't match: ${actualEvmToken} vs ${expectedAddress}`
        );
    }
}

async function main() {
    const program = new Command();

    program
        .requiredOption('-d, --tx-dir <dir>', 'Directory containing transaction receipts')
        .requiredOption('-n, --near-token <address>', 'NEAR token address')
        .requiredOption('-t, --token-tx <hash>', 'EVM token deployment transaction hash')
        .requiredOption('-c, --chain-kind <chain-kind>', 'Chain kind')
        .requiredOption('-l, --near-locker <address>', 'NEAR locker address')
        .parse(process.argv);

    const options = program.opts();
    const config: TokenBridgeVerificationConfig = {
        receiptsDir: options.txDir,
        nearTokenAddress: options.nearToken,
        evmTokenTxHash: options.tokenTx,
        chainKind: options.chainKind,
        nearLockerAddress: options.nearLocker
    };

    try {
        const transactions = await loadTransactions(config.receiptsDir);

        await verifyTransactions(transactions);
        console.log('Transactions verified successfully!');

        const evmTokenAddress = await getEvmTokenAddressFromTx(config.evmTokenTxHash);

        await verifyTokenMetadata(config.nearTokenAddress, evmTokenAddress);
        console.log('Token metadata verified successfully!');

        await verifyLockerTokenAddress(config.nearLockerAddress, config.nearTokenAddress, evmTokenAddress, config.chainKind);
        console.log('Locker token address verified successfully!');

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