import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import {parsePubkey} from './keyParser';

export function installRegisterMintCLI(program: Command) {
  program
    .command('register-mint')
    .description('Register a solana mint on near')
    .requiredOption('--mint <pubkey>', 'Mint address')
    .option('--override-authority', 'Override mint authority')
    .option('--name <string>', 'Override token name')
    .option('--symbol <string>', 'Override token symbol')
    .action(
      async ({
        mint,
        name,
        symbol,
        overrideAuthority,
      }: {
        mint: string;
        name: string;
        symbol: string;
        overrideAuthority?: string;
      }) => {
        const {sdk} = getContext();
        const mintPk = await parsePubkey(mint);
        const overrideAuthorityPk = overrideAuthority
          ? await parsePubkey(overrideAuthority)
          : null;
        const {instructions, signers} = await sdk.registerMint({
          mint: mintPk,
          name,
          symbol,
          overrideAuthority: overrideAuthorityPk,
        });
        await executeTx({instructions, signers});
      },
    );
}
