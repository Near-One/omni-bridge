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
    .requiredOption('--destination-nonce <string>', 'Nonce')
    .requiredOption('--origin-chain <string>', 'Origin chain')
    .requiredOption('--origin-nonce <string>', 'Origin nonce')
    .requiredOption('--amount <number>', 'Amount')
    .option('--recipient <pubkey>', 'Recipient')
    .option('--signature <string>', 'Signature')
    .action(
      async ({
        token,
        mint,
        destinationNonce,
        originChain,
        originNonce,
        amount,
        recipient,
        signature,
      }: {
        token?: string;
        mint?: string;
        destinationNonce: string;
        originChain: string;
        originNonce: string;
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
          destinationNonce: new BN(destinationNonce),
          transferId: {
            originChain: parseInt(originChain),
            originNonce: new BN(originNonce),
          },
          amount: new BN(amount),
          recipient: recipientPk,
          signature: signature ? JSON.parse(signature) : new Array(65).fill(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
