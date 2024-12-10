import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installInitTransferCLI(program: Command) {
  program
    .command('init-transfer')
    .description('Init transfer')
    .option('--token <string>', 'Near token address')
    .option('--mint <pubkey>', 'Mint address')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <address>', 'Recipient')
    .option('--fee <number>', 'Fee', '0')
    .option('--native-fee', 'Use native fee', '0')
    .action(
      async ({
        token,
        mint,
        amount,
        recipient,
        fee,
        nativeFee,
      }: {
        token?: string;
        mint?: string;
        amount: string;
        recipient: string;
        fee: string;
        nativeFee: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = mint ? await parsePubkey(mint) : undefined;
        const {instructions, signers} = await sdk.initTransfer({
          mint: mintPk,
          token,
          amount: new BN(amount),
          recipient,
          fee: new BN(fee),
          nativeFee: new BN(nativeFee),
        });
        await executeTx({instructions, signers});
      },
    );
}
