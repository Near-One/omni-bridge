import {AnchorProvider} from '@coral-xyz/anchor';
import {PublicKey, Transaction} from '@solana/web3.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import * as data from '../../src/data/user/initTransfer';
import {BN} from 'bn.js';
import {getAssociatedTokenAddressSync} from '@solana/spl-token';

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

  it('Inits native transfer', async () => {
    const {
      mint,
      amount,
      recipient,
      fee,
      nativeFee,
      owner,
      ownerBalance,
      tokenBalance,
      vaultBalance,
    } = data.native;

    const tx = new Transaction();
    const {instructions, signers, message} = await sdk.initTransfer({
      mint: mint.mint,
      amount,
      user: owner.publicKey,
      recipient,
      fee,
      nativeFee,
    });
    tx.add(...instructions);

    await expect(
      sdk.provider.sendAndConfirm!(tx, [...signers, owner]),
    ).resolves.toBeTruthy();

    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) =>
          OmniBridgeSolanaSDK.parseWormholeMessage(data.subarray(95)),
        ),
    ).resolves.toStrictEqual({
      messageType: 'initTransfer',
      sender: owner.publicKey,
      mint: mint.mint,
      nonce: expect.any(BN),
      amount,
      fee,
      nativeFee,
      recipient,
      messageData: Buffer.alloc(0),
    });

    await expect(
      sdk.provider.connection
        .getTokenAccountBalance(
          getAssociatedTokenAddressSync(mint.mint, owner.publicKey),
        )
        .then(({value}) => value.amount),
    ).resolves.toStrictEqual(tokenBalance.sub(amount).toString());
    await expect(
      sdk.provider.connection
        .getTokenAccountBalance(sdk.vaultId({mint: mint.mint})[0])
        .then(({value}) => value.amount),
    ).resolves.toStrictEqual(vaultBalance.add(amount).toString());

    await expect(
      sdk.provider.connection.getBalance(owner.publicKey),
    ).resolves.toStrictEqual(ownerBalance.sub(nativeFee).toNumber());
  });

  it('Inits bridged transfer', async () => {
    const {
      nearToken,
      mint,
      amount,
      recipient,
      fee,
      nativeFee,
      owner,
      ownerBalance,
      tokenBalance,
    } = data.bridged;
    const [mintId] = sdk.wrappedMintId({token: nearToken});

    const tx = new Transaction();
    const {instructions, signers, message} = await sdk.initTransfer({
      token: nearToken,
      amount,
      user: owner.publicKey,
      recipient,
      fee,
      nativeFee,
    });
    tx.add(...instructions);

    await expect(
      sdk.provider.sendAndConfirm!(tx, [...signers, owner]),
    ).resolves.toBeTruthy();

    await expect(
      sdk.provider.connection
        .getAccountInfo(message)
        .then(({data}) =>
          OmniBridgeSolanaSDK.parseWormholeMessage(data.subarray(95)),
        ),
    ).resolves.toStrictEqual({
      messageType: 'initTransfer',
      sender: owner.publicKey,
      mint: mintId,
      nonce: expect.any(BN),
      amount,
      fee,
      nativeFee,
      recipient,
      messageData: Buffer.alloc(0),
    });

    await expect(
      sdk.provider.connection
        .getTokenAccountBalance(
          getAssociatedTokenAddressSync(mintId, owner.publicKey),
        )
        .then(({value}) => value.amount),
    ).resolves.toStrictEqual(tokenBalance.sub(amount).toString());
    await expect(
      sdk.provider.connection
        .getTokenSupply(mintId)
        .then(({value}) => value.amount),
    ).resolves.toStrictEqual(mint.supply.sub(amount).toString());

    await expect(
      sdk.provider.connection.getBalance(owner.publicKey),
    ).resolves.toStrictEqual(ownerBalance.sub(nativeFee).toNumber());
  });
});
