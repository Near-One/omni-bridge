import "dotenv/config";
import {Command} from "commander";
import {loadTransactions, verifyTransactions} from "../lib/common";
import {VerificationError} from "../lib/types";

async function main() {
    const program = new Command();

    program
        .requiredOption(
            "-d, --tx-dir <dir>",
            "Directory containing transaction receipts",
        )
        .parse(process.argv);

    const options = program.opts();

    try {
        const transactions = await loadTransactions(options.txDir);

        await verifyTransactions(transactions);
        console.log("Transactions verified successfully!");
    } catch (error) {
        if (error instanceof VerificationError) {
            console.error("Verification failed:", error.message);
        } else {
            console.error("Unexpected error:", error);
        }
        process.exit(1);
    }
}

main();
