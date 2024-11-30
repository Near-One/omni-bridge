import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as logMetadata from './logMetadata';
import {Umi} from '@metaplex-foundation/umi';

export {logMetadata};

export function setup({sdk, umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  return Promise.all([logMetadata.setup({sdk, umi})]);
}
