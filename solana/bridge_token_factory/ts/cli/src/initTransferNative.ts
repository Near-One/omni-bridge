import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installInitTransferNativeCLI(program: Command) {
  program
    .command('init-transfer-native')
    .description('Init native transfer')
    .requiredOption('--mint <pubkey>', 'Mint address')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <address>', 'Recipient')
    .option('--fee <number>', 'Fee', '0')
    .action(
      async ({
        mint,
        amount,
        recipient,
        fee,
      }: {
        mint: string;
        amount: string;
        recipient: string;
        fee?: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = await parsePubkey(mint);
        const {instructions, signers} = await sdk.initTransferNative({
          mint: mintPk,
          amount: new BN(amount),
          recipient,
          fee: fee ? new BN(fee) : new BN(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
