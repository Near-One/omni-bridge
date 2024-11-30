import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import {parsePubkey} from './keyParser';

export function installLogMetadataCLI(program: Command) {
  program
    .command('log-metadata')
    .description('Register a solana mint on near')
    .requiredOption('--mint <pubkey>', 'Mint address')
    .action(async ({mint}: {mint: string}) => {
      const {sdk} = getContext();
      const mintPk = await parsePubkey(mint);

      const {instructions, signers} = await sdk.logMetadata({
        mint: mintPk,
      });
      await executeTx({instructions, signers});
    });
}
