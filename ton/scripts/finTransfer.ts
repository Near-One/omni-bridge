import type { NetworkProvider } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Relayer (= you) submits a NEAR-MPC-signed TransferMessagePayload to TON's
// fin_transfer. The locker verifies via ECRECOVER and releases native TON /
// TEP-74 / bridge-minted mint based on the payload's token address.
//
//   bunx blueprint run finTransfer --testnet --mnemonic -- \
//       --payload <hex-of-TransferMessagePayload> \
//       --sigR <hex> \
//       --sigS <hex> \
//       --sigV <0|1>
//
//   Optional:
//       --value <nanoTON>   attached value for locker compute (default 0.01 TON)
//       --queryId <uint64>  default 0
//
// Extracting the fields from NEAR's `SignTransferEvent.signature`
// (`SignatureResponse`), per `near/omni-types/src/mpc_types.rs`:
//
//   sigR = big_r.affine_point[2..]     // drop the 1-byte SEC1 prefix (0x02/0x03)
//   sigS = s.scalar                    // verbatim
//   sigV = recovery_id                 // 0 or 1 as returned; pass directly
//
// TON's ECRECOVER takes v ∈ {0,1} (raw y-parity). EVM wants v+27; TON doesn't.
// Recovery_id is authoritative — use it directly even if the affine_point
// prefix byte suggests otherwise.

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const payloadHex = mustArg(parsed, 'payload').replace(/^0x/, '');
    const sigR = BigInt(prefixedHex(mustArg(parsed, 'sigR')));
    const sigS = BigInt(prefixedHex(mustArg(parsed, 'sigS')));
    const sigV = Number(mustArg(parsed, 'sigV'));
    const value = parsed.value ? BigInt(parsed.value) : toNano('0.01');
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;

    if (sigV !== 0 && sigV !== 1) {
        throw new Error(
            `sigV must be 0 or 1 (y-parity); got ${sigV}. Use SignatureResponse.recovery_id directly (NOT big_r's SEC1 prefix, NOT v-27 from an EVM adapter).`,
        );
    }

    const payload = Buffer.from(payloadHex, 'hex');
    if (payload.length === 0) throw new Error('empty payload');

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendFinTransfer(provider.sender(), {
        value,
        queryId,
        sigR,
        sigS,
        sigV,
        payload,
    });

    console.log('fin_transfer sent');
    console.log('  payload bytes =', payload.length);
    console.log('  sigV          =', sigV);
    console.log('  attached      =', value.toString(), 'nanoTON');
    console.log();
    console.log(
        `Watch locker at https://testnet.tonviewer.com/${lockerAddr.toString({ testOnly: true })}`,
    );
    console.log('  Success → FinTransferEvent (0x99000002) ext-out');
    console.log('  Stuck   → FinTransferStuckEvent (0x99000020) ext-out (downstream send bounced)');
}

function prefixedHex(s: string): string {
    return s.startsWith('0x') ? s : `0x${s}`;
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
