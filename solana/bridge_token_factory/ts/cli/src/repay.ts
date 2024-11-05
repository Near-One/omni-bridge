import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';

export function installRepayCLI(program: Command) {
  program
    .command('repay')
    .description('Repay')
    .requiredOption('--token <pubkey>', 'Mint address')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <address>', 'Recipient')
    .action(
      async ({
        token,
        amount,
        recipient,
      }: {
        token: string;
        amount: string;
        recipient: string;
      }) => {
        const {sdk} = getContext();
        const {instructions, signers} = await sdk.repay({
          token,
          amount: new BN(amount),
          recipient,
        });
        await executeTx({instructions, signers});
      },
    );
}
