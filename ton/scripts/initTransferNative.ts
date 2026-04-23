import type { NetworkProvider } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Send native TON to the locker with a cross-chain recipient. Triggers an
// `InitTransferEvent` ext-out on the locker. Relayer (= you) reads that event
// and submits a mock proof to NEAR.
//
//   bunx blueprint run initTransferNative --testnet --mnemonic -- \
//       --amount 500000000 \
//       --recipient near:alice.testnet
//
//   Optional:
//       --fee <nanoTON>          default 0
//       --nativeFee <nanoTON>    default 0
//       --message <utf8-string>  default empty
//       --queryId <uint64>       default 0
//
// Attached value = amount + nativeFee + gas cushion (~0.1 TON for locker compute).

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const amount = BigInt(mustArg(parsed, 'amount'));
    const recipient = mustArg(parsed, 'recipient');
    const fee = BigInt(parsed.fee ?? '0');
    const nativeFee = BigInt(parsed.nativeFee ?? '0');
    const message = parsed.message;
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    const value = amount + nativeFee + toNano('0.1');

    await bridge.sendInitTransferNative(provider.sender(), {
        value,
        queryId,
        amount,
        fee,
        nativeFee,
        recipient: Buffer.from(recipient, 'utf8'),
        message: message ? Buffer.from(message, 'utf8') : undefined,
    });

    console.log('init_transfer_native sent');
    console.log('  amount     =', amount.toString(), 'nanoTON');
    console.log('  fee        =', fee.toString());
    console.log('  nativeFee  =', nativeFee.toString());
    console.log('  recipient  =', recipient);
    console.log('  attached   =', value.toString(), 'nanoTON');
    console.log();
    console.log('Watch tonviewer for the InitTransferEvent ext-out (op=0x99000001):');
    console.log(`  https://testnet.tonviewer.com/${lockerAddr.toString({ testOnly: true })}`);
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set — export the locker address first');
    return v;
}
