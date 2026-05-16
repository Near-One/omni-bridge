import { randomBytes } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { SigningKey, getBytes, hexlify, keccak256 } from 'ethers';

// Generates (or loads) a secp256k1 "mock MPC" key for local testnet use,
// then prints the 20-byte Ethereum-style derived address that the TON locker
// and the NEAR bridge must both trust.
//
// Usage:
//   npx blueprint run deriveMpc
//   # -> prints MPC_PRIV_KEY and MPC_DERIVED_ADDR; also saves key to ../.mpc-key

const MPC_FILE = resolve(__dirname, '..', '.mpc-key');

export async function run() {
    let priv = process.env.MPC_PRIV_KEY;
    if (!priv && existsSync(MPC_FILE)) {
        priv = readFileSync(MPC_FILE, 'utf8').trim();
        console.log(`(loaded existing key from ${MPC_FILE})`);
    }
    if (!priv) {
        priv = `0x${randomBytes(32).toString('hex')}`;
        writeFileSync(MPC_FILE, priv, { mode: 0o600 });
        console.log(`Generated new MPC key → ${MPC_FILE}`);
        console.log('   (keep this file safe; anyone with it can release funds on TON)');
    }

    const sk = new SigningKey(priv);
    const xy = getBytes(sk.publicKey).slice(1); // strip the 0x04 uncompressed prefix
    const hashBytes = getBytes(keccak256(xy));
    const addr20 = BigInt(hexlify(hashBytes.slice(12)));

    console.log();
    console.log('MPC_PRIV_KEY         =', priv);
    console.log(`MPC_DERIVED_ADDR     = 0x${addr20.toString(16).padStart(40, '0')}`);
    console.log('MPC_DERIVED_ADDR_DEC =', addr20.toString());
    console.log();
    console.log('Add to your .env:');
    console.log(`   MPC_PRIV_KEY=${priv}`);
    console.log(`   MPC_DERIVED_ADDR=0x${addr20.toString(16).padStart(40, '0')}`);
}
