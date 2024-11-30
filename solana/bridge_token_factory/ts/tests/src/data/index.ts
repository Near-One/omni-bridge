import * as admin from './admin';
import * as user from './user';
import {programIdKp} from './common';
import * as main from './main';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {Umi} from '@metaplex-foundation/umi';
export {admin, user, programIdKp, main};

export function setup({sdk, umi}: {sdk: OmniBridgeSolanaSDK; umi: Umi}) {
  return Promise.all([main.setup(sdk), user.setup({sdk, umi})]);
}
