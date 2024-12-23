/* eslint-disable n/no-unsupported-features/node-builtins */
import {Keypair, PublicKey} from '@solana/web3.js';
import BN from 'bn.js';
import {writeFile} from 'fs/promises';
import {
  mintAccount,
  MintAccountArgs,
  systemAccount,
  tokenAccount,
  TokenAccountArgs,
} from '../../utils';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';

export const mint: MintAccountArgs = {
  mint: new PublicKey('6evD8mRBKMmca4iFF9SS9a8sdoaSzpg2tXt8z5J6HTts'),
  decimals: 6,
  supply: new BN('4393331823'),
  mintAuthority: new PublicKey('4BBVnRsidYiAQhg4j3xuWgUS9VDuVYNXgPEQ2p3o2NuC'),
};

export const owner = Keypair.fromSecretKey(
  new Uint8Array([
    1, 80, 208, 118, 238, 134, 103, 184, 50, 27, 38, 59, 43, 244, 148, 191, 199,
    229, 212, 188, 156, 239, 82, 3, 55, 53, 178, 67, 167, 74, 150, 75, 202, 239,
    111, 201, 126, 93, 172, 25, 217, 236, 208, 193, 145, 175, 90, 128, 184, 135,
    185, 42, 255, 33, 135, 188, 176, 159, 123, 84, 82, 221, 180, 29,
  ]),
); // EfB7cLDAC7xq29hfEEJARmzYDPBHnijL9JCc1xMkXSur

export const ownerBalance = new BN('79083248088');

export const tokenBalance = new BN('9258884');
export const vaultBalance = new BN('7552');

export const amount = new BN('92690');

export const recipient = 'The target';
export const fee = new BN(2);
export const nativeFee = new BN(4);

export async function setup({sdk}: {sdk: OmniBridgeSolanaSDK}) {
  const token: TokenAccountArgs = {
    mint: mint.mint,
    owner: owner.publicKey,
    amount: tokenBalance,
  };

  const vault: TokenAccountArgs = {
    address: sdk.vaultId({mint: mint.mint})[0],
    mint: mint.mint,
    owner: sdk.authority()[0],
    amount: vaultBalance,
  };

  await writeFile(
    '../../tests/assets/user/initTransfer/native/mint.json',
    JSON.stringify(mintAccount(mint), undefined, 2),
  );
  await writeFile(
    '../../tests/assets/user/initTransfer/native/vault.json',
    JSON.stringify(tokenAccount(vault), undefined, 2),
  );
  await writeFile(
    '../../tests/assets/user/initTransfer/native/owner.json',
    JSON.stringify(
      systemAccount({address: owner.publicKey, balance: ownerBalance}),
      undefined,
      2,
    ),
  );
  await writeFile(
    '../../tests/assets/user/initTransfer/native/token.json',
    JSON.stringify(tokenAccount(token), undefined, 2),
  );
}
