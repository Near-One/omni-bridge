import {IdlAccounts, IdlEvents, Program, Provider} from '@coral-xyz/anchor';

import {BridgeTokenFactory} from './bridge_token_factory';
import * as BridgeTokenFactoryIdl from './bridge_token_factory.json';
import {
  PublicKey,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  TransactionInstruction,
} from '@solana/web3.js';

import BN from 'bn.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';

export type ConfigAccount = IdlAccounts<BridgeTokenFactory>['config'];

const MPL_PROGRAM_ID = new PublicKey(
  'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s',
);

export class OmniBridgeSolanaSDK {
  public readonly wormholeProgramId: PublicKey;
  public readonly program: Program<BridgeTokenFactory>;

  public static readonly CONFIG_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(({name}) => name === 'CONFIG_SEED')!
        .value,
    ),
  );

  public static readonly AUTHORITY_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(
        ({name}) => name === 'AUTHORITY_SEED',
      )!.value,
    ),
  );

  public static readonly VAULT_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(({name}) => name === 'VAULT_SEED')!
        .value,
    ),
  );

  public static readonly MESSAGE_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(({name}) => name === 'MESSAGE_SEED')!
        .value,
    ),
  );

  public static readonly USED_NONCES_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(
        ({name}) => name === 'USED_NONCES_SEED',
      )!.value,
    ),
  );

  public static readonly WRAPPED_MINT_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(
        ({name}) => name === 'WRAPPED_MINT_SEED',
      )!.value,
    ),
  );

  public static readonly DEFAULT_ADMIN = new PublicKey(
    BridgeTokenFactoryIdl.constants.find(
      ({name}) => name === 'DEFAULT_ADMIN',
    )!.value,
  );

  public static readonly USED_NONCES_PER_ACCOUNT = parseInt(
    BridgeTokenFactoryIdl.constants.find(
      ({name}) => name === 'USED_NONCES_PER_ACCOUNT',
    )!.value,
  );

  configId(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.CONFIG_SEED],
      this.programId,
    );
  }

  authority(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.AUTHORITY_SEED],
      this.programId,
    );
  }

  messageId({sequenceNumber}: {sequenceNumber: BN}): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.MESSAGE_SEED, sequenceNumber.toBuffer('le', 8)],
      this.programId,
    );
  }

  wormholeBridgeId(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from('Bridge', 'utf-8')],
      this.wormholeProgramId,
    );
  }

  wormholeFeeCollectorId(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from('fee_collector', 'utf-8')],
      this.wormholeProgramId,
    );
  }

  wormholeSequenceId(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from('Sequence', 'utf-8'), this.configId()[0].toBuffer()],
      this.wormholeProgramId,
    );
  }

  wrappedMintId({token}: {token: string}): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.WRAPPED_MINT_SEED, Buffer.from(token, 'utf-8')],
      this.programId,
    );
  }

  usedNoncesId({nonce}: {nonce: BN}): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [
        OmniBridgeSolanaSDK.USED_NONCES_SEED,
        nonce
          .divn(OmniBridgeSolanaSDK.USED_NONCES_PER_ACCOUNT)
          .toBuffer('le', 16),
      ],
      this.programId,
    );
  }

  constructor({
    provider,
    wormholeProgramId,
  }: {
    provider: Provider;
    wormholeProgramId: PublicKey;
  }) {
    this.wormholeProgramId = wormholeProgramId;
    this.program = new Program(
      BridgeTokenFactoryIdl as BridgeTokenFactory,
      provider,
    );
  }

  get programId(): PublicKey {
    return this.program.programId;
  }

  get provider(): Provider {
    return this.program.provider;
  }

  async fetchConfig(): Promise<ConfigAccount> {
    return await this.program.account.config.fetch(this.configId()[0]);
  }

  async fetchNextSequenceNumber(): Promise<BN> {
    const {data} = (await this.provider.connection.getAccountInfo(
      this.wormholeSequenceId()[0],
    ))!;
    return new BN(data.subarray(0, 8), 'le').addn(1);
  }

  async initialize({
    payer,
    nearBridge,
  }: {
    payer?: PublicKey;
    nearBridge: number[];
  }): Promise<TransactionInstruction> {
    return await this.program.methods
      .initialize(nearBridge)
      .accountsStrict({
        config: this.configId()[0],
        authority: this.authority()[0],
        payer: payer || this.provider.publicKey!,
        clock: SYSVAR_CLOCK_PUBKEY,
        rent: SYSVAR_RENT_PUBKEY,
        admin: OmniBridgeSolanaSDK.DEFAULT_ADMIN,
        wormholeBridge: this.wormholeBridgeId()[0],
        wormholeFeeCollector: this.wormholeFeeCollectorId()[0],
        wormholeSequence: this.wormholeSequenceId()[0],
        wormholeMessage: this.messageId({sequenceNumber: new BN(1)})[0],
        systemProgram: SystemProgram.programId,
        wormholeProgram: this.wormholeProgramId,
      })
      .instruction();
  }

  async deployToken({
    token,
    name,
    symbol,
    decimals,
    signature,
    payer,
    sequenceNumber,
  }: {
    token: string;
    name: string;
    symbol: string;
    decimals: number;
    signature: number[];
    payer?: PublicKey;
    sequenceNumber?: BN;
  }) {
    const [mint] = this.wrappedMintId({token});
    const [metadata] = PublicKey.findProgramAddressSync(
      [
        Buffer.from('metadata', 'utf-8'),
        MPL_PROGRAM_ID.toBuffer(),
        mint.toBuffer(),
      ],
      MPL_PROGRAM_ID,
    );
    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    return await this.program.methods
      .deployToken({
        metadata: {
          token,
          name,
          symbol,
          decimals,
        },
        signature,
      })
      .accountsStrict({
        authority: this.authority()[0],
        wormhole: {
          payer: payer || this.provider.publicKey!,
          config: this.configId()[0],
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: this.messageId({sequenceNumber})[0],
        },
        metadata,
        systemProgram: SystemProgram.programId,
        mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenMetadataProgram: MPL_PROGRAM_ID,
      })
      .instruction();
  }

  async finalizeDeposit({
    nonce,
    token,
    amount,
    recipient,
    feeRecipient = null,
    signature,
    payer,
    sequenceNumber,
  }: {
    nonce: BN;
    token: string;
    amount: BN;
    recipient: PublicKey;
    feeRecipient?: string | null;
    signature: number[];
    payer?: PublicKey;
    sequenceNumber?: BN;
  }) {
    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    const [config] = this.configId();
    const [usedNonces] = this.usedNoncesId({nonce});
    const [mint] = this.wrappedMintId({token});
    const tokenAccount = getAssociatedTokenAddressSync(mint, recipient, true);

    return await this.program.methods
      .finalizeDeposit({
        payload: {
          nonce,
          token,
          amount,
          feeRecipient,
        },
        signature,
      })
      .accountsStrict({
        config,
        usedNonces,
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: this.messageId({sequenceNumber})[0],
          payer: payer || this.provider.publicKey!,
        },
        recipient,
        authority: this.authority()[0],
        mint,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenAccount,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .instruction();
  }
}
