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
    .action(
      async ({
        token,
        mint,
        amount,
        recipient,
        fee,
      }: {
        token?: string;
        mint?: string;
        amount: string;
        recipient: string;
        fee?: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = mint ? await parsePubkey(mint) : undefined;
        const {instructions, signers} = await sdk.initTransfer({
          mint: mintPk,
          token,
          amount: new BN(amount),
          recipient,
          fee: fee ? new BN(fee) : new BN(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
