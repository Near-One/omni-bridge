import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';

export function installDeployTokenCLI(program: Command) {
  program
    .command('deploy-token')
    .description('Deploy the token')
    .requiredOption('--token <pubkey>', 'Mint address')
    .requiredOption('--name <string>', 'Token name')
    .requiredOption('--symbol <string>', 'Token symbol')
    .requiredOption('--decimals <number>', 'Token decimals', parseInt)
    .option('--signature <string>', 'Signature')
    .action(
      async ({
        token,
        name,
        symbol,
        decimals,
        signature,
      }: {
        token: string;
        name: string;
        symbol: string;
        decimals: number;
        signature?: string;
      }) => {
        const {sdk} = getContext();
        const {instructions, signers} = await sdk.deployToken({
          token,
          name,
          symbol,
          decimals,
          signature: signature ? JSON.parse(signature) : new Array(65).fill(0),
        });
        await executeTx({instructions, signers});
      },
    );
}
