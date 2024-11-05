import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installFinalizeDepositCLI(program: Command) {
  program
    .command('finalize-deposit')
    .description('Finalize the deposit')
    .requiredOption('--token <pubkey>', 'Mint address')
    .requiredOption('--nonce <string>', 'Nonce')
    .requiredOption('--amount <number>', 'Amount')
    .requiredOption('--recipient <pubkey>', 'Recipient')
    .option('--signature <string>', 'Signature')
    .action(
      async ({
        token,
        nonce,
        amount,
        recipient,
        signature,
      }: {
        token: string;
        nonce: string;
        amount: string;
        recipient: string;
        signature?: string;
      }) => {
        const {sdk} = getContext();
        const {instructions, signers} = await sdk.finalizeDeposit({
          token,
          nonce: new BN(nonce),
          amount: new BN(amount),
          recipient: await parsePubkey(recipient),
          signature: signature ? JSON.parse(signature) : new Array(65).fill(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
