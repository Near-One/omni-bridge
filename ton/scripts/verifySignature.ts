import type { NetworkProvider } from '@ton/blueprint';
import { Signature, keccak256, recoverAddress } from 'ethers';
import { mustArg, parseArgs } from './_argv';

// Off-chain dry-run of what the TON locker's `verifyMpcSignature` does:
//   1. keccak256(payload)
//   2. ECRECOVER(hash, v, r, s)  -> uncompressed pubkey
//   3. keccak256(pubkey)[12..32] -> Ethereum-style address
//
// If the recovered address matches `--expected` (your nearBridgeDerivedAddr),
// the signature IS cryptographically valid. That means the locker is rejecting
// for a non-crypto reason (ERR_BAD_DESTINATION_CHAIN, out of gas, etc.).
//
// If it DOESN'T match, one of:
//   - the payload bytes differ from what NEAR hashed (encoding mismatch)
//   - sigR/sigS were copied wrong
//   - sigV (recovery_id) is wrong
//   - the TON locker was configured with the wrong nearBridgeDerivedAddr
//
//   bunx blueprint run verifySignature --testnet -- \
//       --payload <hex> --sigR <hex> --sigS <hex> --sigV <0|1> \
//       --expected <0x...>

export async function run(_provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const payloadHex = mustArg(parsed, 'payload').replace(/^0x/, '');
    const r = `0x${mustArg(parsed, 'sigR').replace(/^0x/, '')}`;
    const s = `0x${mustArg(parsed, 'sigS').replace(/^0x/, '')}`;
    const v = Number(mustArg(parsed, 'sigV'));
    const expected = mustArg(parsed, 'expected').toLowerCase();

    if (v !== 0 && v !== 1) {
        throw new Error(`sigV must be 0 or 1 (y-parity); got ${v}`);
    }

    const payload = Buffer.from(payloadHex, 'hex');
    const hash = keccak256(payload);

    console.log();
    console.log('Input:');
    console.log('  payload length:', payload.length, 'bytes');
    console.log('  keccak256     :', hash);
    console.log('  sigR          :', r);
    console.log('  sigS          :', s);
    console.log('  sigV (yParity):', v);
    console.log();

    // ethers Signature expects yParity ∈ {0,1} or v ∈ {27,28}. We use yParity.
    const sig = Signature.from({ r, s, yParity: v as 0 | 1 });
    const recovered = recoverAddress(hash, sig).toLowerCase();

    console.log('Recovered address :', recovered);
    console.log('Expected address  :', expected.startsWith('0x') ? expected : `0x${expected}`);
    console.log();

    const expectedNormalized = expected.startsWith('0x') ? expected : `0x${expected}`;
    if (recovered === expectedNormalized) {
        console.log('✓ Signature is VALID for this payload + nearBridgeDerivedAddr');
        console.log('  → the locker is rejecting for a non-crypto reason.');
        console.log('  → suspect gas (bump --value), or a non-sig assertion');
        console.log('    (ERR_BAD_DESTINATION_CHAIN=105, ERR_NONCE_USED=103, etc.)');
    } else {
        console.log('✗ Signature does NOT match the expected address.');
        console.log('  Possible causes (in order of likelihood):');
        console.log('    1. Payload encoding differs from what NEAR hashed.');
        console.log('    2. nearBridgeDerivedAddr configured on the locker is for the');
        console.log('       WRONG NEAR account — you need the derived pubkey for the');
        console.log('       omni-bridge contract that called sign_transfer, not some');
        console.log('       other account.');
        console.log('    3. Wrong sigV (try flipping 0 ↔ 1 to see if that recovers).');
        console.log();
        console.log('  Try the other y-parity:');
        const flipped = Signature.from({ r, s, yParity: (1 - v) as 0 | 1 });
        const flippedRecovered = recoverAddress(hash, flipped).toLowerCase();
        console.log('    with sigV =', 1 - v, '→', flippedRecovered);
        if (flippedRecovered === expectedNormalized) {
            console.log('    ✓ That matches! Re-submit with --sigV', 1 - v);
        } else {
            console.log('    ✗ Still no match. Payload bytes are almost certainly wrong,');
            console.log('      OR nearBridgeDerivedAddr on the locker is for a different');
            console.log('      NEAR account.');
            console.log();
            console.log('  To verify (3): query for the CORRECT predecessor:');
            console.log('    near contract call-function as-read-only v1.signer-prod.testnet \\');
            console.log('      derived_public_key \\');
            console.log(
                '      json-args \'{"path":"bridge-1","predecessor":"<NEAR-bridge-account>"}\' \\',
            );
            console.log('      network-config testnet now');
            console.log('  Then nearPubkeyToAddress on the result and compare to the');
            console.log("  locker's getState() → nearBridgeDerivedAddr.");
        }
    }
}
