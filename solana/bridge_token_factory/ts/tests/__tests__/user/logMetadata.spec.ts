import {AnchorProvider} from '@coral-xyz/anchor';
import {PublicKey, Transaction} from '@solana/web3.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as data from '../../src/data/user/logMetadata';

describe('logMetadata', () => {
  let sdk: OmniBridgeSolanaSDK;

  beforeAll(() => {
    sdk = new OmniBridgeSolanaSDK({
      provider: AnchorProvider.local(),
      wormholeProgramId: new PublicKey(
        'worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth',
      ),
    });
  });

  it('Logs empty strings if no MPL account exists', async () => {
    const {mint} = data.noMetadata;
    const tx = new Transaction();
    const {instructions, signers, message} = await sdk.logMetadata({
      mint: mint.mint,
    });
    tx.add(...instructions);

    await expect(
      sdk.provider.sendAndConfirm!(tx, [...signers]),
    ).resolves.toBeTruthy();

    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) =>
          OmniBridgeSolanaSDK.parseWormholeMessage(data.subarray(95)),
        ),
    ).resolves.toStrictEqual({
      messageType: 'logMetadata',
      mint: mint.mint,
      name: '',
      symbol: '',
      decimals: mint.decimals,
    });

    await expect(
      sdk.provider.connection
        .getParsedAccountInfo(sdk.vaultId({mint: mint.mint})[0])
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        .then(({value}) => (value.data as any).parsed.info),
    ).resolves.toStrictEqual({
      isNative: false,
      mint: mint.mint.toBase58(),
      owner: sdk.authority()[0].toBase58(),
      state: 'initialized',
      tokenAmount: {
        amount: '0',
        decimals: mint.decimals,
        uiAmount: 0,
        uiAmountString: '0',
      },
    });
  });

  it('Logs MLP metadata', async () => {
    const {mint, metadata} = data.mplMetadata;
    const tx = new Transaction();
    const {instructions, signers, message} = await sdk.logMetadata({
      mint: mint.mint,
    });
    tx.add(...instructions);

    await expect(
      sdk.provider.sendAndConfirm!(tx, [...signers]),
    ).resolves.toBeTruthy();

    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) =>
          OmniBridgeSolanaSDK.parseWormholeMessage(data.subarray(95)),
        ),
    ).resolves.toStrictEqual({
      messageType: 'logMetadata',
      mint: mint.mint,
      name: metadata.name,
      symbol: metadata.symbol,
      decimals: mint.decimals,
    });

    await expect(
      sdk.provider.connection
        .getParsedAccountInfo(sdk.vaultId({mint: mint.mint})[0])
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        .then(({value}) => (value.data as any).parsed.info),
    ).resolves.toStrictEqual({
      isNative: false,
      mint: mint.mint.toBase58(),
      owner: sdk.authority()[0].toBase58(),
      state: 'initialized',
      tokenAmount: {
        amount: '0',
        decimals: mint.decimals,
        uiAmount: 0,
        uiAmountString: '0',
      },
    });
  });
});
