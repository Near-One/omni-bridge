import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import BN from 'bn.js';
import {parsePubkey} from './keyParser';

export function installFinalizeTransferBridgedCLI(program: Command) {
  program
    .command('finalize-transfer-bridged')
    .description('Finalize bridged transfer')
    .requiredOption('--token <string>', 'Near token address')
    .requiredOption('--nonce <string>', 'Nonce')
    .requiredOption('--amount <number>', 'Amount')
    .option('--recipient <pubkey>', 'Recipient')
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
        recipient?: string;
        signature?: string;
      }) => {
        const {sdk} = getContext();
        const recipientPk = recipient
          ? await parsePubkey(recipient)
          : sdk.provider.publicKey!;
        const {instructions, signers} = await sdk.finalizeTransferBridged({
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
