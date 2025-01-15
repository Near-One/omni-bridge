import {Keypair, PublicKey} from '@solana/web3.js';
import expandTilde from 'expand-tilde';
import fs from 'mz/fs';
import {getContext} from './context';

export async function parseKeypair(input: string) {
  if (!input.startsWith('[')) {
    input = await fs.readFile(expandTilde(input), 'utf-8');
  }
  return Keypair.fromSecretKey(new Uint8Array(JSON.parse(input)));
}

export async function parsePubkey(pubkeyOrPath: string): Promise<PublicKey> {
  const {keyMap} = getContext();
  const key = keyMap.get(pubkeyOrPath);
  if (key) {
    return key;
  }
  try {
    return new PublicKey(pubkeyOrPath);
  } catch (err) {
    const keypair = await parseKeypair(pubkeyOrPath);
    return keypair.publicKey;
  }
}

export async function parsePubkeyOrKeypair(
  pubkeyOrPath: string,
): Promise<PublicKey | Keypair> {
  try {
    return new PublicKey(pubkeyOrPath);
  } catch (err) {
    return await parseKeypair(pubkeyOrPath);
  }
}
