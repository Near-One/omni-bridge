import type { NetworkProvider } from '@ton/blueprint';
import { mustArg, parseArgs } from './_argv';

// Takes a NEAR `LogMetadataEvent` JSON (as logged by `log_metadata`) and
// prints a ready-to-paste deployJetton command — Borsh-encoded MetadataPayload
// + extracted sigR/sigS/sigV.
//
//   bunx blueprint run formatLogMetadataEvent --testnet -- \
//       --event '{"LogMetadataEvent":{...}}'
//
//   # Or read from file:
//   bunx blueprint run formatLogMetadataEvent --testnet -- \
//       --eventFile /path/to/event.json
//
// Borsh MetadataPayload layout (from near/omni-types/src/lib.rs:685):
//   prefix  : u8                (PayloadType::Metadata = 1)
//   token   : string (u32 LE len + utf8)
//   name    : string
//   symbol  : string
//   decimals: u8

const PAYLOAD_METADATA = 1;

interface LogMetadataEventEnvelope {
    LogMetadataEvent: {
        signature: {
            big_r: { affine_point: string };
            s: { scalar: string };
            recovery_id: number;
        };
        metadata_payload: {
            prefix: string;
            token: string;
            name: string;
            symbol: string;
            decimals: number;
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

    const envelope: LogMetadataEventEnvelope = JSON.parse(raw);
    const ev = envelope.LogMetadataEvent;
    if (!ev) {
        throw new Error('expected a `{"LogMetadataEvent": {...}}` envelope');
    }

    const affineHex = ev.signature.big_r.affine_point.replace(/^0x/, '');
    if (affineHex.length !== 66) {
        throw new Error(`big_r.affine_point must be 33 bytes hex (got ${affineHex.length / 2})`);
    }
    const sigR = affineHex.slice(2);
    const sigS = ev.signature.s.scalar.replace(/^0x/, '');
    const sigV = ev.signature.recovery_id;
    if (sigV !== 0 && sigV !== 1) {
        throw new Error(`recovery_id must be 0 or 1, got ${sigV}`);
    }

    const mp = ev.metadata_payload;
    if (mp.prefix !== 'Metadata') {
        throw new Error(`expected prefix=Metadata, got ${mp.prefix}`);
    }
    if (mp.decimals < 0 || mp.decimals > 255) {
        throw new Error(`decimals must fit in u8, got ${mp.decimals}`);
    }

    const parts: Buffer[] = [
        Buffer.from([PAYLOAD_METADATA]),
        borshString(mp.token),
        borshString(mp.name),
        borshString(mp.symbol),
        Buffer.from([mp.decimals & 0xff]),
    ];

    const payload = Buffer.concat(parts);

    console.log();
    console.log('Reconstructed MetadataPayload:');
    console.log('  length :', payload.length, 'bytes');
    console.log('  hex    :', payload.toString('hex'));
    console.log();
    console.log('Signature components:');
    console.log('  sigR =', sigR);
    console.log('  sigS =', sigS);
    console.log('  sigV =', sigV);
    console.log();
    console.log('Paste into deployJetton:');
    console.log('  bunx blueprint run deployJetton --testnet --mnemonic -- \\');
    console.log(`      --payload ${payload.toString('hex')} \\`);
    console.log(`      --sigR ${sigR} \\`);
    console.log(`      --sigS ${sigS} \\`);
    console.log(`      --sigV ${sigV}`);
}

function borshString(s: string): Buffer {
    const bytes = Buffer.from(s, 'utf8');
    const len = Buffer.alloc(4);
    len.writeUInt32LE(bytes.length, 0);
    return Buffer.concat([len, bytes]);
}
