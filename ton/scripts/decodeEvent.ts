import type { NetworkProvider } from '@ton/blueprint';
import { Cell, type Slice } from '@ton/core';
import { Opcodes } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Decode a single locker ext-out event body. Copy the BoC hex from
// tonviewer's "out messages" → "body (raw)" column, or fetch via
// toncenter's /api/v3/transactions?include_msgs=true and pull the body.
//
//   bunx blueprint run decodeEvent --testnet -- --boc <hex>
//
// Decodes: InitTransfer, FinTransfer, DeployToken, LogMetadata,
// RegisterJetton, FinTransferStuck, BridgeMintedRefunded,
// DeployTokenFailed, LogMetadataFailed, PauseStateChanged, AdminTransferred.

export async function run(_provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    // const hex = mustArg(parsed, 'boc').replace(/^0x/, '');
    // const cell = Cell.fromBoc(Buffer.from(hex, 'hex'))[0];
    const raw = mustArg(parsed, 'boc');
    const buf = /^[0-9a-fA-F]+$/.test(raw)
        ? Buffer.from(raw.replace(/^0x/, ''), 'hex') // plain hex
        : Buffer.from(raw, 'base64'); // te6cck… base64
    const cell = Cell.fromBoc(buf)[0];
    const s = cell.beginParse();
    const op = s.loadUint(32);

    console.log();
    console.log(`opcode: 0x${op.toString(16).padStart(8, '0')}`);

    switch (op) {
        case Opcodes.EVENT_INIT_TRANSFER:
            return decodeInitTransfer(s);
        case Opcodes.EVENT_FIN_TRANSFER:
            return decodeFinTransfer(s);
        case Opcodes.EVENT_DEPLOY_TOKEN:
            return decodeDeployToken(s);
        case Opcodes.EVENT_LOG_METADATA:
            return decodeLogMetadata(s);
        case Opcodes.EVENT_REGISTER_JETTON:
            return decodeRegisterJetton(s);
        case Opcodes.EVENT_FIN_STUCK:
            return decodeFinTransferStuck(s);
        case Opcodes.EVENT_BRIDGE_MINTED_REFUND:
            return decodeBridgeMintedRefund(s);
        case Opcodes.EVENT_DEPLOY_TOKEN_FAILED:
            return decodeDeployTokenFailed(s);
        case Opcodes.EVENT_LOG_METADATA_FAILED:
            return decodeLogMetadataFailed(s);
        case Opcodes.EVENT_PAUSE_STATE:
            return decodePauseState(s);
        case Opcodes.EVENT_ADMIN:
            return decodeAdmin(s);
        default:
            console.log('  (unknown opcode)');
    }
}

function decodeInitTransfer(s: Slice) {
    console.log('event: InitTransfer');
    console.log('  sender       =', s.loadAddress().toString({ testOnly: true }));
    console.log('  tokenMaster  =', formatOptionalAddr(s));
    console.log('  originNonce  =', s.loadUintBig(64).toString());
    console.log('  amount       =', s.loadUintBig(128).toString());
    console.log('  fee          =', s.loadUintBig(128).toString());
    console.log('  nativeFee    =', s.loadUintBig(128).toString());
    console.log('  recipient    =', refToUtf8(s));
    console.log('  message      =', refToUtf8(s));
}

function decodeFinTransfer(s: Slice) {
    console.log('event: FinTransfer');
    console.log('  originChain       =', s.loadUint(8));
    console.log('  originNonce       =', s.loadUintBig(64).toString());
    console.log('  destinationNonce  =', s.loadUintBig(64).toString());
    console.log(`  recipient (hex)   = 0x${s.loadUintBig(256).toString(16).padStart(64, '0')}`);
    console.log('  amount            =', s.loadUintBig(128).toString());
    console.log('  feeRecipient      =', refToUtf8(s));
    console.log('  message           =', refToUtf8(s));
}

function decodeDeployToken(s: Slice) {
    console.log('event: DeployToken');
    console.log('  master      =', s.loadAddress().toString({ testOnly: true }));
    console.log('  lockerJw    =', s.loadAddress().toString({ testOnly: true }));
    console.log('  decimals    =', s.loadUint(8));
    console.log('  nearTokenId =', refToUtf8(s));
}

function decodeLogMetadata(s: Slice) {
    console.log('event: LogMetadata');
    console.log('  master =', s.loadAddress().toString({ testOnly: true }));
}

function decodeRegisterJetton(s: Slice) {
    console.log('event: RegisterJetton');
    console.log('  master   =', s.loadAddress().toString({ testOnly: true }));
    console.log('  lockerJw =', s.loadAddress().toString({ testOnly: true }));
    console.log('  kind     =', s.loadUint(8), '(0=BRIDGE_MINTED, 1=LOCKED_NATIVE)');
}

function decodeFinTransferStuck(s: Slice) {
    console.log('event: FinTransferStuck');
    console.log('  destinationNonce =', s.loadUintBig(64).toString());
    console.log('  bouncedFrom      =', s.loadAddress().toString({ testOnly: true }));
}

function decodeBridgeMintedRefund(s: Slice) {
    console.log('event: BridgeMintedRefunded');
    console.log('  user    =', s.loadAddress().toString({ testOnly: true }));
    console.log('  master  =', s.loadAddress().toString({ testOnly: true }));
    console.log('  amount  =', s.loadUintBig(128).toString());
    console.log('  queryId =', s.loadUintBig(64).toString());
}

function decodeDeployTokenFailed(s: Slice) {
    console.log('event: DeployTokenFailed');
    console.log('  master   =', s.loadAddress().toString({ testOnly: true }));
    console.log('  lockerJw =', s.loadAddress().toString({ testOnly: true }));
}

function decodeLogMetadataFailed(s: Slice) {
    console.log('event: LogMetadataFailed');
    console.log('  master =', s.loadAddress().toString({ testOnly: true }));
}

function decodePauseState(s: Slice) {
    console.log('event: PauseStateChanged');
    console.log('  oldFlags =', s.loadUint(8));
    console.log('  newFlags =', s.loadUint(8));
}

function decodeAdmin(s: Slice) {
    console.log('event: AdminTransferred');
    console.log('  oldAdmin =', s.loadAddress().toString({ testOnly: true }));
    console.log('  newAdmin =', s.loadAddress().toString({ testOnly: true }));
}

function refToUtf8(s: Slice): string {
    if (s.remainingRefs === 0) return '(no ref)';
    const ref = s.loadRef().beginParse();
    const bits = ref.remainingBits;
    const bytes = Math.floor(bits / 8);
    const buf = Buffer.alloc(bytes);
    for (let i = 0; i < bytes; i++) buf[i] = ref.loadUint(8);
    try {
        return JSON.stringify(buf.toString('utf8'));
    } catch {
        return `0x${buf.toString('hex')}`;
    }
}

function formatOptionalAddr(s: Slice): string {
    const addr = s.loadAddress();
    return addr.toString({ testOnly: true });
}
