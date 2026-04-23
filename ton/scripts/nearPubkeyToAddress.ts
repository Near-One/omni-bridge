import type { NetworkProvider } from '@ton/blueprint';
import { decodeBase58, getBytes, hexlify, keccak256, toBeArray } from 'ethers';
import { mustArg, parseArgs } from './_argv';

// Convert a NEAR-format secp256k1 public key (as returned by
// `v1.signer-prod.testnet::derived_public_key`) into the 20-byte Ethereum-style
// address that the TON locker's `nearBridgeDerivedAddr` should match.
//
//   bunx blueprint run nearPubkeyToAddress --testnet -- \
//       --pubkey secp256k1:57GfZid...hqKqYQ3hFnYrf6xuD8DRz4TY2C71qkL6P1gnGq6AErz
//
// NEAR returns "secp256k1:<base58 of 64 uncompressed pubkey bytes>" — note
// there's NO 0x04 prefix, so the base58 decodes to exactly 64 bytes
// (x || y, 32 bytes each). keccak256 that and take the last 20 bytes.

export async function run(_provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const raw = mustArg(parsed, 'pubkey');
    const base58 = raw.includes(':') ? raw.split(':', 2)[1] : raw;

    const pubkey = padTo64(toBeArray(decodeBase58(base58)));
    if (pubkey.length !== 64) {
        throw new Error(`expected 64 pubkey bytes, got ${pubkey.length}`);
    }

    const hashed = getBytes(keccak256(pubkey));
    const addr20 = hashed.slice(12);
    const hex = hexlify(addr20);

    console.log();
    console.log('NEAR pubkey:', raw);
    console.log();
    console.log('Ethereum-style derived address:');
    console.log('  hex       =', hex);
    console.log('  decimal   =', BigInt(hex).toString());
    console.log();
    console.log('Paste into ton/.env as:');
    console.log(`  MPC_DERIVED_ADDR=${hex}`);
}

function padTo64(bytes: Uint8Array): Uint8Array {
    if (bytes.length === 64) return bytes;
    if (bytes.length > 64) {
        throw new Error(`unexpected pubkey length ${bytes.length} (max 64)`);
    }
    // decodeBase58 → toBeArray strips leading zeros; re-pad on the left.
    const out = new Uint8Array(64);
    out.set(bytes, 64 - bytes.length);
    return out;
}
