// eslint-disable-next-line n/no-unpublished-import
import {expect} from '@jest/globals';
import {PublicKey} from '@solana/web3.js';
import BN from 'bn.js';

/**
 * Equality testers for jest to compare BN and PublicKey.
 */
expect.addEqualityTesters([
  (a, b) => {
    if (BN.isBN(a)) {
      return a.eq(b as BN);
    }
    return undefined;
  },
  (a, b) => {
    if (((a && a[Symbol.toStringTag]) || '').startsWith('PublicKey')) {
      return a.equals(b as PublicKey);
    }
    return undefined;
  },
]);
