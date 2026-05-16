import type { NetworkProvider } from '@ton/blueprint';
import { Address } from '@ton/core';
import { keccak256 } from 'ethers';
import { mustArg, parseArgs } from './_argv';

// Takes a NEAR `SignTransferEvent` JSON (as logged by `sign_transfer`) and
// prints a ready-to-paste finTransfer command — including the Borsh-encoded
// payload hex and signature components correctly extracted.
//
// Pipe the event JSON file or paste it literally:
//
//   bunx blueprint run formatSignTransferEvent --testnet -- \
//       --event '{"SignTransferEvent":{...}}'
//
//   # Or read from a file:
//   bunx blueprint run formatSignTransferEvent --testnet -- \
//       --eventFile /path/to/event.json
//
// Rules applied (per near/omni-types/src/mpc_types.rs):
//   sigR = big_r.affine_point[2..]    (drop 1-byte SEC1 prefix)
//   sigS = s.scalar
//   sigV = recovery_id                (authoritative; NOT derived from prefix)

const CHAIN_NEAR = 1;
const CHAIN_TON = 12;
const PAYLOAD_TRANSFER_MESSAGE = 0;

interface SignTransferEventEnvelope {
    SignTransferEvent: {
        signature: {
            big_r: { affine_point: string };
            s: { scalar: string };
            recovery_id: number;
        };
        message_payload: {
            prefix: string;
            destination_nonce: number | string;
            transfer_id: {
                origin_chain: string;
                origin_nonce: number | string;
            };
            token_address: string;
            amount: string;
            recipient: string;
            fee_recipient: string | null;
            message: number[] | string;
        };
    };
}

export async function run(_provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    let raw: string;
    if (parsed.eventFile) {
        const fs = await import('node:fs');
        raw = fs.readFileSync(parsed.eventFile, 'utf8');
    } else {
        raw = mustArg(parsed, 'event');
    }

    const envelope: SignTransferEventEnvelope = JSON.parse(raw);
    const ev = envelope.SignTransferEvent;
    if (!ev) {
        throw new Error('expected a `{"SignTransferEvent": {...}}` envelope');
    }

    // --- Extract signature parts
    const affineHex = ev.signature.big_r.affine_point.replace(/^0x/, '');
    if (affineHex.length !== 66) {
        throw new Error(`big_r.affine_point must be 33 bytes hex (got ${affineHex.length / 2})`);
    }
    const sigR = affineHex.slice(2); // drop SEC1 prefix byte
    const sigS = ev.signature.s.scalar.replace(/^0x/, '');
    const sigV = ev.signature.recovery_id;
    if (sigV !== 0 && sigV !== 1) {
        throw new Error(`recovery_id must be 0 or 1, got ${sigV}`);
    }

    // --- Rebuild the exact bytes NEAR hashed
    const mp = ev.message_payload;
    if (mp.prefix !== 'TransferMessage') {
        throw new Error(`expected prefix=TransferMessage, got ${mp.prefix}`);
    }
    if (mp.transfer_id.origin_chain !== 'Near') {
        throw new Error(
            `only NEAR-origin transfers supported here (got origin_chain=${mp.transfer_id.origin_chain})`,
        );
    }

    const destNonce = BigInt(mp.destination_nonce);
    const originNonce = BigInt(mp.transfer_id.origin_nonce);
    const amount = BigInt(mp.amount);

    const tokenHash = omniTonToHash(mp.token_address);
    const recipientHash = omniTonToHash(mp.recipient);

    const message: Buffer = Array.isArray(mp.message)
        ? Buffer.from(mp.message)
        : Buffer.from(mp.message, 'utf8');

    const parts: Buffer[] = [
        Buffer.from([PAYLOAD_TRANSFER_MESSAGE]),
        u64le(destNonce),
        Buffer.from([CHAIN_NEAR]),
        u64le(originNonce),
        Buffer.from([CHAIN_TON]),
        tokenHash,
        u128le(amount),
        Buffer.from([CHAIN_TON]),
        recipientHash,
    ];

    if (mp.fee_recipient === null) {
        parts.push(Buffer.from([0]));
    } else {
        parts.push(Buffer.from([1]));
        const s = Buffer.from(mp.fee_recipient, 'utf8');
        parts.push(u32le(s.length));
        parts.push(s);
    }

    // V2 (non-empty message): append u32 LE length + bytes
    if (message.length > 0) {
        parts.push(u32le(message.length));
        parts.push(message);
    }

    const payload = Buffer.concat(parts);
    const hash = keccak256(payload);

    console.log();
    console.log('Reconstructed payload:');
    console.log('  length      :', payload.length, 'bytes');
    console.log('  hex         :', payload.toString('hex'));
    console.log('  keccak256   :', hash);
    console.log();
    console.log('Signature components (NOT the EVM-style 65-byte packed form):');
    console.log('  sigR  =', sigR);
    console.log('  sigS  =', sigS);
    console.log('  sigV  =', sigV, '(recovery_id, used directly on TON)');
    console.log();
    console.log('Paste into finTransfer:');
    console.log('  bunx blueprint run finTransfer --testnet --mnemonic -- \\');
    console.log(`      --payload ${payload.toString('hex')} \\`);
    console.log(`      --sigR ${sigR} \\`);
    console.log(`      --sigS ${sigS} \\`);
    console.log(`      --sigV ${sigV}`);
}

function omniTonToHash(s: string): Buffer {
    // Accepts "ton:<EQ|kQ|0Q|UQ>..." — we just want the 32-byte hash.
    const prefix = 'ton:';
    if (!s.startsWith(prefix)) {
        throw new Error(`expected ton:<addr> OmniAddress, got "${s}"`);
    }
    const userFriendly = s.slice(prefix.length);
    return Buffer.from(Address.parse(userFriendly).hash);
}

function u64le(v: bigint): Buffer {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(v, 0);
    return b;
}

function u128le(v: bigint): Buffer {
    const b = Buffer.alloc(16);
    for (let i = 0; i < 16; i++) b[i] = Number((v >> BigInt(i * 8)) & 0xffn);
    return b;
}

function u32le(v: number): Buffer {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(v, 0);
    return b;
}
