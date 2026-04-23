import type { NetworkProvider } from '@ton/blueprint';
import { Address, Cell, beginCell, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Submit a NEAR-MPC-signed MetadataPayload to TON's deploy_token. The locker
// derives a deterministic BridgeJettonMaster address from the payload + our
// jetton master code, deploys it, and registers it in the shelf.
//
//   bunx blueprint run deployJetton --testnet --mnemonic -- \
//       --payload <hex-of-MetadataPayload> \
//       --sigR <hex> \
//       --sigS <hex> \
//       --sigV <0|1>
//
//   Optional:
//       --contentHex <hex-of-BoC>   TEP-64 content cell for the new master
//                                   (default: empty cell)
//       --value <nanoTON>           attached value (default 0.5 TON — must
//                                   cover master deploy + JW compute)
//       --queryId <uint64>          default 0
//
// Subtract 27 from NEAR MPC's sig_v byte.

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const payloadHex = mustArg(parsed, 'payload').replace(/^0x/, '');
    const sigR = BigInt(prefixedHex(mustArg(parsed, 'sigR')));
    const sigS = BigInt(prefixedHex(mustArg(parsed, 'sigS')));
    const sigV = Number(mustArg(parsed, 'sigV'));
    const value = parsed.value ? BigInt(parsed.value) : toNano('0.5');
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;

    if (sigV !== 0 && sigV !== 1) {
        throw new Error(`sigV must be 0 or 1; got ${sigV} (subtract 27 from NEAR MPC's v byte)`);
    }

    const payload = Buffer.from(payloadHex, 'hex');
    if (payload.length === 0) throw new Error('empty payload');

    let contentRef: Cell | undefined;
    if (parsed.contentHex) {
        const hex = parsed.contentHex.replace(/^0x/, '');
        contentRef = Cell.fromBoc(Buffer.from(hex, 'hex'))[0];
    } else {
        contentRef = beginCell().endCell();
    }

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendDeployToken(provider.sender(), {
        value,
        queryId,
        sigR,
        sigS,
        sigV,
        metadataPayload: payload,
        contentRef,
    });

    console.log('deploy_token sent');
    console.log('  payload bytes =', payload.length);
    console.log('  sigV          =', sigV);
    console.log('  contentRef    =', parsed.contentHex ? 'user-provided' : 'empty cell');
    console.log('  attached      =', value.toString(), 'nanoTON');
    console.log();
    console.log('Watch locker ext-out for:');
    console.log('  DeployTokenEvent (0x99000003)    — master registered + deploy fired');
    console.log('  DeployTokenFailedEvent (0x99000023) — deploy bounced; shelf entry reverted');
}

function prefixedHex(s: string): string {
    return s.startsWith('0x') ? s : `0x${s}`;
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
