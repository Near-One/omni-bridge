import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';

export function installInitTransferBridgedCLI(program: Command) {
  program
    .command('init-transfer-bridged')
    .description('Init bridged transfer')
    .requiredOption('--token <string>', 'Token address')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <address>', 'Recipient')
    .option('--fee <number>', 'Fee', '0')
    .action(
      async ({
        token,
        amount,
        recipient,
        fee,
      }: {
        token: string;
        amount: string;
        recipient: string;
        fee?: string;
      }) => {
        const {sdk} = getContext();
        const {instructions, signers} = await sdk.initTransferBridged({
          token,
          amount: new BN(amount),
          recipient,
          fee: fee ? new BN(fee) : new BN(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
