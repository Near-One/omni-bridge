/* eslint-disable n/no-unsupported-features/node-builtins */
import {PublicKey} from '@solana/web3.js';
import BN from 'bn.js';
import {writeFile} from 'fs/promises';
import {mintAccount} from '../../common';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';

export const mint = {
  mint: new PublicKey('7K92wLm7Gfy3tgBVmxGyA5ueyYnx5gML1SHsAonby8ay'),
  decimals: 6,
  supply: new BN('32769834'),
  mintAuthority: new PublicKey('Fi9u6RrCyU78F7Nv7hBruQn4xbqti9jNj1xwvE2FkmY8'),
};

export async function setup(_sdk: OmniBridgeSolanaSDK) {
  await writeFile(
    '../../tests/assets/user/logMetadata/noMetadata/mint.json',
    JSON.stringify(await mintAccount(mint), undefined, 2),
  );
}
