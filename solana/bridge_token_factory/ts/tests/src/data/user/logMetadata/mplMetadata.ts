/* eslint-disable n/no-unsupported-features/node-builtins */
import {PublicKey} from '@solana/web3.js';
import BN from 'bn.js';
import {writeFile} from 'fs/promises';
import {metadataAccount, mintAccount} from '../../common';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {Program} from '@coral-xyz/anchor';
import {
  getMetadataAccountDataSerializer,
  Metadata,
  MetadataAccountDataArgs,
} from '@metaplex-foundation/mpl-token-metadata';
import {publicKey} from '@metaplex-foundation/umi-public-keys';
import {Umi} from '@metaplex-foundation/umi';

export const mint = {
  mint: new PublicKey('2iSA1BhgQpLPvZeQTJ6quTaGWRYc6AWcVYn2L4CaMxVJ'),
  decimals: 9,
  supply: new BN('833232200'),
  mintAuthority: new PublicKey('Ck7CqBfeFdbbg68iUCBXiZ5dEyqpQXzSnN7E2CZGtGqP'),
};

export const metadata: MetadataAccountDataArgs = {
  updateAuthority: publicKey('Ck7CqBfeFdbbg68iUCBXiZ5dEyqpQXzSnN7E2CZGtGqP'),
  mint: publicKey(mint.mint),
  name: 'Name',
  symbol: 'Smb',
  uri: 'Uri',
  sellerFeeBasisPoints: 0,
  creators: [],
  primarySaleHappened: false,
  isMutable: false,
  editionNonce: 0,
  tokenStandard: null,
  collection: null,
  uses: null,
  collectionDetails: null,
  programmableConfig: null,
};

export async function setup({umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  await writeFile(
    '../../tests/assets/user/logMetadata/mplMetadata/mint.json',
    JSON.stringify(mintAccount(mint), undefined, 2),
  );

  await writeFile(
    '../../tests/assets/user/logMetadata/mplMetadata/metadata.json',
    JSON.stringify(metadataAccount({metadata, umi}), undefined, 2),
  );
}
