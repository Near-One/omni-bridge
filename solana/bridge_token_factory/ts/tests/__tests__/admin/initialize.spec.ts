import {AnchorProvider, setProvider} from '@coral-xyz/anchor';
import {PublicKey, Transaction} from '@solana/web3.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {simple} from '../../src/data/admin/initialize';
import {programIdKp} from '../../src/data';
import {BN} from 'bn.js';

describe('initialize', () => {
  let sdk: OmniBridgeSolanaSDK;

  beforeAll(() => {
    sdk = new OmniBridgeSolanaSDK({
      provider: AnchorProvider.local(),
      wormholeProgramId: new PublicKey(
        'worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth',
      ),
    });
  });

  it('Works in simple case', async () => {
    const tx = new Transaction();
    const {instructions, signers} = await sdk.initialize({
      nearBridge: simple.nearBridge,
      admin: simple.admin,
    });
    tx.add(...instructions);

    await expect(
      sdk.provider.sendAndConfirm!(tx, [...signers, programIdKp]),
    ).resolves.toBeTruthy();

    await expect(sdk.fetchConfig()).resolves.toStrictEqual({
      admin: simple.admin,
      maxUsedNonce: new BN(0),
      derivedNearBridgeAddress: simple.nearBridge,
      bumps: {
        config: sdk.configId()[1],
        authority: sdk.authority()[1],
        wormhole: {
          bridge: sdk.wormholeBridgeId()[1],
          feeCollector: sdk.wormholeFeeCollectorId()[1],
          sequence: sdk.wormholeSequenceId()[1],
        },
      },
    });
  });
});
