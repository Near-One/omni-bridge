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

export type ConfigAccount = IdlAccounts<BridgeTokenFactory>['config'];

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

  public static readonly DEFAULT_ADMIN = new PublicKey(
    BridgeTokenFactoryIdl.constants.find(
      ({name}) => name === 'DEFAULT_ADMIN',
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
}
