import {Keypair} from '@solana/web3.js';
import BN from 'bn.js';
import {ConfigAccount, OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {getMinimumBalanceForRentExemption, omniBridgeAccount} from '../common';
import {Base64} from 'js-base64';
// eslint-disable-next-line n/no-unsupported-features/node-builtins
import {writeFile} from 'fs/promises';

export const adminKP = Keypair.fromSecretKey(
  Uint8Array.from([
    90, 63, 54, 31, 49, 125, 220, 17, 9, 70, 65, 129, 11, 206, 1, 114, 105, 83,
    26, 66, 67, 16, 75, 172, 223, 250, 181, 226, 178, 140, 223, 241, 155, 82,
    66, 23, 164, 100, 126, 190, 164, 222, 58, 234, 27, 108, 90, 122, 53, 248,
    84, 200, 33, 3, 111, 76, 67, 153, 65, 195, 182, 163, 102, 68,
  ]),
); // BTJwgEmqRt4QjHtL7WwhKS9JifjqQQdLshv8EtucHdyM

export function config(sdk: OmniBridgeSolanaSDK): ConfigAccount {
  return {
    admin: adminKP.publicKey,
    maxUsedNonce: new BN(10),
    derivedNearBridgeAddress: new Array(64).fill(0),
    bumps: {
      config: sdk.configId()[1],
      authority: sdk.authority()[1],
      solVault: sdk.solVault()[1],
      wormhole: {
        bridge: sdk.wormholeBridgeId()[1],
        feeCollector: sdk.wormholeFeeCollectorId()[1],
        sequence: sdk.wormholeSequenceId()[1],
      },
    },
  };
}

export async function setup(sdk: OmniBridgeSolanaSDK) {
  await writeFile(
    '../../tests/assets/main/config.json',
    JSON.stringify(
      await omniBridgeAccount({
        sdk,
        account: config(sdk),
        accountType: 'config',
      }),
      undefined,
      2,
    ),
  );

  await writeFile(
    '../../tests/assets/main/sequence.json',
    JSON.stringify(
      {
        pubkey: sdk.wormholeSequenceId()[0].toBase58(),
        account: {
          lamports: getMinimumBalanceForRentExemption(8),
          data: [Base64.fromUint8Array(new BN(0).toBuffer('le', 8)), 'base64'],
          owner: sdk.wormholeProgramId.toBase58(),
          executable: false,
          rentEpoch: 0,
        },
      },
      undefined,
      2,
    ),
  );
}
