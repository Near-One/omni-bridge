import 'dotenv/config';
import { Command } from 'commander';
import { getLockerTokenAddress } from '../lib/near';

async function main() {
    const program = new Command();

    program
        .requiredOption('-n, --near-token <address>', 'NEAR token address')
        .requiredOption('-c, --chain-kind <chain-kind>', 'Chain kind')
        .requiredOption('-l, --near-locker <address>', 'NEAR locker address')
        .parse(process.argv);

    const options = program.opts();

    try {
        const evmTokenAddress = await getLockerTokenAddress(options.nearLocker, options.nearToken, options.chainKind);
        console.log(evmTokenAddress.split(':')[1]);
    } catch (error) {
        process.exit(1);
    }
}

main();