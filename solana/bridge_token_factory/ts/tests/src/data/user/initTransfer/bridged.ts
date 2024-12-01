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

export const nearToken = 'The token';

export const mint: Omit<Omit<MintAccountArgs, 'mint'>, 'mintAuthority'> = {
  decimals: 6,
  supply: new BN('78352821234'),
};

export const owner = Keypair.fromSecretKey(
  new Uint8Array([
    200, 192, 104, 178, 210, 27, 92, 22, 180, 120, 58, 14, 205, 78, 60, 148, 4,
    219, 162, 33, 205, 127, 39, 246, 185, 231, 102, 182, 115, 241, 76, 131, 38,
    1, 238, 254, 122, 231, 82, 214, 231, 186, 76, 90, 80, 54, 165, 173, 42, 0,
    183, 131, 113, 105, 3, 160, 250, 119, 24, 144, 197, 14, 86, 217,
  ]),
); // 3ZNCkTisRtqjvwtDqqB9Vk1KKBTW6QEPybQB4aAHbKag

export const ownerBalance = new BN('6789883836');

export const tokenBalance = new BN('780639829');

export const amount = new BN('825784');

export const recipient = 'The Recepient';
export const fee = new BN(344);
export const nativeFee = new BN(22);

export async function setup({sdk}: {sdk: OmniBridgeSolanaSDK}) {
  const [mintId] = sdk.wrappedMintId({token: nearToken});
  const token: TokenAccountArgs = {
    mint: mintId,
    owner: owner.publicKey,
    amount: tokenBalance,
  };

  await writeFile(
    '../../tests/assets/user/initTransfer/bridged/mint.json',
    JSON.stringify(
      mintAccount({mint: mintId, mintAuthority: sdk.authority()[0], ...mint}),
      undefined,
      2,
    ),
  );

  await writeFile(
    '../../tests/assets/user/initTransfer/bridged/owner.json',
    JSON.stringify(
      systemAccount({address: owner.publicKey, balance: ownerBalance}),
      undefined,
      2,
    ),
  );
  await writeFile(
    '../../tests/assets/user/initTransfer/bridged/token.json',
    JSON.stringify(tokenAccount(token), undefined, 2),
  );
}
