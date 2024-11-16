import {Keypair} from '@solana/web3.js';
import BN from 'bn.js';

BN.prototype.toJSON = function () {
  return this.toString();
};

const ACCOUNT_STORAGE_OVERHEAD = 128;
const DEFAULT_LAMPORTS_PER_BYTE_YEAR = Math.floor(
  ((1_000_000_000 / 100) * 365) / (1024 * 1024),
);
const DEFAULT_EXEMPTION_THRESHOLD = 2.0;

export function getMinimumBalanceForRentExemption(bytes: number) {
  return (
    (ACCOUNT_STORAGE_OVERHEAD + bytes) *
    DEFAULT_LAMPORTS_PER_BYTE_YEAR *
    DEFAULT_EXEMPTION_THRESHOLD
  );
}

export const programIdKp = Keypair.fromSecretKey(
  Uint8Array.from([
    225, 34, 97, 224, 178, 48, 236, 237, 241, 233, 132, 211, 119, 49, 88, 177,
    166, 27, 217, 184, 217, 106, 155, 103, 153, 230, 150, 210, 195, 72, 9, 57,
    38, 35, 227, 206, 5, 147, 218, 190, 207, 202, 141, 133, 60, 31, 98, 56, 108,
    157, 32, 138, 168, 136, 244, 155, 16, 157, 174, 238, 124, 95, 238, 37,
  ]),
); // 3ZtEZ8xABFbUr4c1FVpXbQiVdqv4vwhvfCc8HMmhEeua
