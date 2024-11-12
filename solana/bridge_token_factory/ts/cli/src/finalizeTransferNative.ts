import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installFinalizeTransferNativeCLI(program: Command) {
  program
    .command('finalize-transfer-native')
    .description('Finalize native transfer')
    .requiredOption('--mint <pubkey>', 'Mint address')
    .requiredOption('--nonce <string>', 'Nonce')
    .requiredOption('--amount <number>', 'Amount')
    .option('--recipient <pubkey>', 'Recipient')
    .option('--signature <string>', 'Signature')
    .action(
      async ({
        mint,
        nonce,
        amount,
        recipient,
        signature,
      }: {
        mint: string;
        nonce: string;
        amount: string;
        recipient?: string;
        signature?: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = await parsePubkey(mint);
        const recipientPk = recipient
          ? await parsePubkey(recipient)
          : sdk.provider.publicKey!;
        const {instructions, signers} = await sdk.finalizeTransferNative({
          mint: mintPk,
          nonce: new BN(nonce),
          amount: new BN(amount),
          recipient: recipientPk,
          signature: signature ? JSON.parse(signature) : new Array(65).fill(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
