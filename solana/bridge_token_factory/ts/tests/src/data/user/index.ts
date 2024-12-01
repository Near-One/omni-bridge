import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as logMetadata from './logMetadata';
import * as initTransfer from './initTransfer';
import {Umi} from '@metaplex-foundation/umi';

export {logMetadata, initTransfer};

export function setup({sdk, umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  return Promise.all([
    logMetadata.setup({sdk, umi}),
    initTransfer.setup({sdk, umi}),
  ]);
}
