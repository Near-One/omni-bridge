import type { NetworkProvider } from '@ton/blueprint';
import { Address } from '@ton/core';
import { parseArgs } from './_argv';

// Prints all encodings (user-friendly + raw) and on-chain state for an address.
// With no args, shows the mnemonic-derived wallet address.
//
//   bunx blueprint run showAddress --testnet --mnemonic
//   bunx blueprint run showAddress --testnet -- --address kQ...

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);

    let addr: Address;
    if (parsed.address) {
        addr = Address.parse(parsed.address);
    } else {
        const sender = provider.sender();
        if (!sender.address) {
            throw new Error('no sender.address — provider not properly initialized');
        }
        addr = sender.address;
    }

    console.log();
    console.log(
        'Bounceable (testnet kQ / mainnet EQ):    ',
        addr.toString({ testOnly: true, bounceable: true }),
    );
    console.log(
        'Non-bounceable (testnet 0Q / mainnet UQ):',
        addr.toString({ testOnly: true, bounceable: false }),
    );
    console.log('Raw:                                     ', addr.toRawString());
    console.log();

    try {
        const state = await provider.provider(addr).getState();
        console.log('Balance:            ', state.balance, 'nanoTON');
        console.log('Deployed:           ', state.state.type === 'active');
    } catch (_e) {
        console.log('(could not fetch state — address may be uninitialized)');
    }
}
