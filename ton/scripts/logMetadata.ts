import type { NetworkProvider } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Permissionless log_metadata: initiate a TEP-89 handshake with a jetton master
// so the locker can discover its own wallet address for that master.
//
//   bunx blueprint run logMetadata --testnet --mnemonic -- \
//       --master EQ<USDT_MASTER>
//
// Required: TON_LOCKER env var set to the locker address (EQ or raw).
// Minimum attached value is 0.2 TON (the locker enforces it). This script
// attaches 0.25 TON to leave margin for the TEP-89 round-trip + refund.
//
// Watch locker's ext-out for `LogMetadataEvent` (0x99000004) — event fires
// only after master replies to `provide_wallet_address`. On bounce (master
// doesn't implement TEP-89), `LogMetadataFailedEvent` (0x99000024) is emitted
// and the fee is refunded.

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const master = Address.parse(mustArg(parsed, 'master'));
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendLogMetadata(provider.sender(), {
        value: toNano('0.25'),
        queryId,
        master,
    });

    console.log('log_metadata sent');
    console.log('  master  =', master.toString({ testOnly: true }));
    console.log('  queryId =', queryId);
    console.log();
    console.log('Waiting for master to reply with take_wallet_address (~5-10s)…');
    console.log('Check pending state:');
    console.log(`  npx blueprint getMethod OmniBridge getPendingRegistration ${master.toString()}`);
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set — export the locker address first');
    return v;
}
