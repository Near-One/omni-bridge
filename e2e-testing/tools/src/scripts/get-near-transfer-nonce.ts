import 'dotenv/config';
import { Command } from 'commander';
import { getNearLog } from '../lib/near';

async function main() {
    const program = new Command();

    program
        .requiredOption('-t, --tx-hash <dir>', 'Transaction hash of init transfer')
        .parse(process.argv);

    const options = program.opts();

    try {
        const initTransferLog = await getNearLog(options.txHash, 1, 0);
        const log = JSON.parse(initTransferLog);
        console.log(log.InitTransferEvent.transfer_message.origin_nonce);
    } catch (error) {
        process.exit(1);
    }
}

main();