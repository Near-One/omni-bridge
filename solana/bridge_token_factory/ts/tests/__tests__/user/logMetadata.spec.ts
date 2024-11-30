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
    const messageData = Buffer.alloc(43);
    messageData.writeInt8(3, 0); // LogMetadata
    messageData.writeInt8(2, 1); // SOLANA_OMNI_BRIDGE_CHAIN_ID
    mint.mint.toBuffer().copy(messageData, 2); // mint
    // Name
    messageData.writeUint32LE(0, 34);
    // Symbol
    messageData.writeUint32LE(0, 38);
    messageData.writeInt8(mint.decimals, 42); // Decimals
    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) => data.subarray(95)),
    ).resolves.toStrictEqual(messageData);

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
    const messageData = Buffer.alloc(
      43 + metadata.name.length + metadata.symbol.length,
    );
    messageData.writeInt8(3, 0); // LogMetadata
    messageData.writeInt8(2, 1); // SOLANA_OMNI_BRIDGE_CHAIN_ID
    mint.mint.toBuffer().copy(messageData, 2); // mint
    let offset = 34;
    // Name
    messageData.writeUint32LE(metadata.name.length, offset);
    offset += 4;
    messageData.write(metadata.name, offset);
    offset += metadata.name.length;
    // Symbol
    messageData.writeUint32LE(metadata.symbol.length, offset);
    offset += 4;
    messageData.write(metadata.symbol, offset);
    offset += metadata.symbol.length;
    messageData.writeInt8(mint.decimals, offset); // Decimals
    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) => data.subarray(95)),
    ).resolves.toStrictEqual(messageData);

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
