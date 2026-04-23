import type { NetworkProvider } from '@ton/blueprint';
import { Address } from '@ton/core';
import { keccak256 } from 'ethers';

// Builds the exact bytes NEAR's omni-bridge signs when you call `sign_transfer`
// for a NEAR→TON transfer. NEAR computes this as:
//
//   borsh::to_vec(&TransferMessagePayloadV1 { prefix, destination_nonce,
//       transfer_id, token_address, amount, recipient, fee_recipient })
//
// (V1 is used when `message` is empty; the full V2 struct appends an Option<bytes>
// but for the common "just move tokens" case you want V1.)
//
// Matches the EVM canonical encoder byte-for-byte (see
// `evm/src/omni-bridge/contracts/OmniBridge.sol:284-303`). Pipe the resulting hex
// straight into `bunx blueprint run finTransfer ... --payload <hex>`.
//
// Tweak the constants below to match your pending NEAR transfer, then:
//
//   bunx blueprint run buildPayload --testnet

// --- ChainKind discriminants (must match near/omni-types/src/lib.rs order)
const CHAIN_NEAR = 1;
const CHAIN_TON = 12;

// --- PayloadType discriminants
const PAYLOAD_TRANSFER_MESSAGE = 0;
// const PAYLOAD_METADATA = 1;        // used by deploy_token, not here

// ============================================================================
// EDIT THESE to match your NEAR transfer
// ============================================================================

const destNonce = 1n; // destination_nonce
const originChain = CHAIN_NEAR; // transfer_id.origin_chain
const originNonce = 1n; // transfer_id.origin_nonce
const tokenAddr = 'EQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAM9c'; // native TON = zero hash
const amount = 1_000_000n; // u128
const recipient = 'EQD7tyOzHs_n3dpvYrbgTWmrQVj0PXj13HQXwZCah4s4GpU7';
const feeRecipient: string | null = null; // None (no fee recipient on NEAR)

// ============================================================================

export async function run(_provider: NetworkProvider) {
    const tokenHash = Address.parse(tokenAddr).hash; // 32 bytes
    const recipientHash = Address.parse(recipient).hash; // 32 bytes

    const parts: Buffer[] = [];

    // prefix: PayloadType::TransferMessage (1-byte enum discriminant)
    parts.push(Buffer.from([PAYLOAD_TRANSFER_MESSAGE]));

    // destination_nonce: u64 LE
    parts.push(u64le(destNonce));

    // transfer_id: struct { origin_chain: u8, origin_nonce: u64 }
    parts.push(Buffer.from([originChain & 0xff]));
    parts.push(u64le(originNonce));

    // token_address: OmniAddress::Ton(TonAddress) — 1-byte variant discriminant + 32 bytes
    parts.push(Buffer.from([CHAIN_TON]));
    parts.push(Buffer.from(tokenHash));

    // amount: u128 LE
    parts.push(u128le(amount));

    // recipient: OmniAddress::Ton — same shape
    parts.push(Buffer.from([CHAIN_TON]));
    parts.push(Buffer.from(recipientHash));

    // fee_recipient: Option<AccountId>
    if (feeRecipient === null) {
        parts.push(Buffer.from([0])); // None
    } else {
        parts.push(Buffer.from([1])); // Some
        const s = Buffer.from(feeRecipient, 'utf8');
        parts.push(u32le(s.length));
        parts.push(s);
    }

    // V1 encoding — NO message field. (If you need to include a non-empty
    // `message: Vec<u8>`, switch to V2: prepend u32 LE length + bytes here.
    // NEAR only uses V2 when `message.is_empty() == false`.)

    const payload = Buffer.concat(parts);
    const hash = keccak256(payload);

    console.log();
    console.log('payload length :', payload.length, 'bytes');
    console.log('payload hex    :', payload.toString('hex'));
    console.log();
    console.log('keccak256 hash :', hash);
    console.log('                 ^ this should match what NEAR MPC signed');
    console.log();
    console.log('Paste into finTransfer:');
    console.log('  bunx blueprint run finTransfer --testnet --mnemonic -- \\');
    console.log(`      --payload ${payload.toString('hex')} \\`);
    console.log('      --sigR <r> --sigS <s> --sigV <v-27>');
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
