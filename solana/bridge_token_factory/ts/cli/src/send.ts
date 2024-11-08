import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installSendCLI(program: Command) {
  program
    .command('send')
    .description('Sends solana token to the near')
    .requiredOption('--mint <pubkey>', 'Mint address')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <address>', 'Recipient')
    .action(
      async ({
        mint,
        amount,
        recipient,
      }: {
        mint: string;
        amount: string;
        recipient: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = await parsePubkey(mint);
        const {instructions, signers} = await sdk.send({
          mint: mintPk,
          amount: new BN(amount),
          recipient,
        });
        await executeTx({instructions, signers});
      },
    );
}
