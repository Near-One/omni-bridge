import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as native from './native';
import * as bridged from './bridged';
import {Umi} from '@metaplex-foundation/umi';

export {native, bridged};

export function setup({sdk, umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  return Promise.all([native.setup({sdk}), bridged.setup({sdk})]);
}
