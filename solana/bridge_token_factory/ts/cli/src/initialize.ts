import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import {parseKeypair} from './keyParser';

export function installInitializeCLI(program: Command) {
  program
    .command('initialize')
    .requiredOption('--program <keypair>', 'Program Keypair')
    .description('Initialize the bridge')
    .action(async ({program}: {program: string}) => {
      const {sdk} = getContext();
      const programKp = await parseKeypair(program);
      const {instructions, signers} = await sdk.initialize({
        nearBridge: new Array(64).fill(0),
      });
      await executeTx({instructions, signers: [...signers, programKp]});
    });
}
