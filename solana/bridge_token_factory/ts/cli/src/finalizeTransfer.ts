import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installFinalizeTransferCLI(program: Command) {
  program
    .command('finalize-transfer')
    .description('Finalize transfer')
    .option('--token <string>', 'Near token address')
    .option('--mint <pubkey>', 'Mint address')
    .requiredOption('--nonce <string>', 'Nonce')
    .requiredOption('--amount <number>', 'Amount')
    .option('--recipient <pubkey>', 'Recipient')
    .option('--signature <string>', 'Signature')
    .action(
      async ({
        token,
        mint,
        nonce,
        amount,
        recipient,
        signature,
      }: {
        token?: string;
        mint?: string;
        nonce: string;
        amount: string;
        recipient?: string;
        signature?: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = mint ? await parsePubkey(mint) : undefined;
        const recipientPk = recipient
          ? await parsePubkey(recipient)
          : sdk.provider.publicKey!;
        const {instructions, signers} = await sdk.finalizeTransfer({
          mint: mintPk,
          token,
          nonce: new BN(nonce),
          amount: new BN(amount),
          recipient: recipientPk,
          signature: signature ? JSON.parse(signature) : new Array(65).fill(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
