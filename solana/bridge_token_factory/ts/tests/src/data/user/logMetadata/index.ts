import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as noMetadata from './noMetadata';
import * as mplMetadata from './mplMetadata';
import {Umi} from '@metaplex-foundation/umi';

export {noMetadata, mplMetadata};

export function setup({sdk, umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  return Promise.all([noMetadata.setup(sdk), mplMetadata.setup({sdk, umi})]);
}
