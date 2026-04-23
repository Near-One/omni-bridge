import { type NetworkProvider, compile } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { parseArgs } from './_argv';

// Admin op: hot-swap the locker's own code via TVM SETCODE. Uses whatever
// the Blueprint compiler produces for the `OmniBridge` contract.
//
//   bunx blueprint run upgradeCode --testnet --mnemonic

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const value = parsed.value ? BigInt(parsed.value) : toNano('0.05');

    const newCode = await compile('OmniBridge');

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendUpgradeCode(provider.sender(), {
        value,
        newCode,
    });

    console.log('upgrade_code sent');
    console.log('  new code hash =', newCode.hash().toString('hex'));
    console.log('  attached      =', value.toString(), 'nanoTON');
    console.log('  locker        =', lockerAddr.toString({ testOnly: true }));
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
