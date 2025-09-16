import "dotenv/config";
import {Command} from "commander";
import {getNearLog} from "../lib/near";

async function main() {
    const program = new Command();

    program
        .requiredOption("-t, --tx-hash <dir>", "Transaction hash of init transfer")
        .option("-r, --receipt-idx <number>", "Receipt index (default: 1)", "1")
        .parse(process.argv);

    const options = program.opts();
    const receiptIdx = parseInt(options.receiptIdx, 10);

    try {
        const initTransferLog = await getNearLog(options.txHash, receiptIdx, 0);
        const log = JSON.parse(initTransferLog);
        console.log(log.InitTransferEvent.transfer_message.origin_nonce);
    } catch (error) {
        process.exit(1);
    }
}

main();
