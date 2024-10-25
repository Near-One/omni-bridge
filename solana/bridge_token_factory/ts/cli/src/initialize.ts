import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';

export function installInitializeCLI(program: Command) {
  program
    .command('initialize')
    .description('Initialize the bridge')
    .action(async () => {
      const {sdk} = getContext();
      const ix = await sdk.initialize({
        nearBridge: new Array(64).fill(0),
      });
      await executeTx({instructions: [ix]});
    });
}
