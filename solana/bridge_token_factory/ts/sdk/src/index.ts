import {IdlAccounts, IdlTypes, Program, Provider} from '@coral-xyz/anchor';

import {BridgeTokenFactory} from './bridge_token_factory';
import * as BridgeTokenFactoryIdl from './bridge_token_factory.json';
import {
  Keypair,
  PublicKey,
  Signer,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  TransactionInstruction,
} from '@solana/web3.js';

import BN from 'bn.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';

export type TransactionData = {
  instructions: TransactionInstruction[];
  signers: Signer[];
};

export type ConfigAccount = IdlAccounts<BridgeTokenFactory>['config'];
export type TransferId = IdlTypes<BridgeTokenFactory>['transferId'];

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

  public static readonly SOL_VAULT_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(
        ({name}) => name === 'SOL_VAULT_SEED',
      )!.value,
    ),
  );

  public static readonly VAULT_SEED = Buffer.from(
    JSON.parse(
      BridgeTokenFactoryIdl.constants.find(({name}) => name === 'VAULT_SEED')!
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

  solVault(): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.SOL_VAULT_SEED],
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

  vaultId({mint}: {mint: PublicKey}): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [OmniBridgeSolanaSDK.VAULT_SEED, mint.toBuffer()],
      this.programId,
    );
  }

  constructor({
    provider,
    programId,
    wormholeProgramId,
  }: {
    provider: Provider;
    programId?: PublicKey;
    wormholeProgramId: PublicKey;
  }) {
    this.wormholeProgramId = wormholeProgramId;
    let idl = BridgeTokenFactoryIdl;
    if (programId) {
      idl = {
        ...BridgeTokenFactoryIdl,
        address: programId.toBase58(),
      };
    }
    this.program = new Program(idl as BridgeTokenFactory, provider);
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
    admin,
  }: {
    payer?: PublicKey;
    nearBridge: number[];
    admin?: PublicKey;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();
    const instruction = await this.program.methods
      .initialize(admin || this.provider.publicKey!, nearBridge)
      .accountsStrict({
        config: this.configId()[0],
        authority: this.authority()[0],
        solVault: this.solVault()[0],
        payer: payer || this.provider.publicKey!,
        clock: SYSVAR_CLOCK_PUBKEY,
        rent: SYSVAR_RENT_PUBKEY,
        wormholeBridge: this.wormholeBridgeId()[0],
        wormholeFeeCollector: this.wormholeFeeCollectorId()[0],
        wormholeSequence: this.wormholeSequenceId()[0],
        wormholeMessage: wormholeMessage.publicKey,
        systemProgram: SystemProgram.programId,
        wormholeProgram: this.wormholeProgramId,
        program: this.programId,
      })
      .instruction();
    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
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
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();
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

    const instruction = await this.program.methods
      .deployToken({
        payload: {
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
          message: wormholeMessage.publicKey,
        },
        metadata,
        systemProgram: SystemProgram.programId,
        mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenMetadataProgram: MPL_PROGRAM_ID,
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  async logMetadata({
    mint,
    sequenceNumber,
    useMetaplex,
    payer,
    token22,
  }: {
    mint: PublicKey;
    sequenceNumber?: BN;
    useMetaplex?: boolean;
    payer?: PublicKey;
    token22?: boolean;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();
    const [config] = this.configId();
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

    if (token22 === undefined) {
      const mintInfo = await this.provider.connection.getAccountInfo(mint);
      token22 = mintInfo?.owner.equals(TOKEN_2022_PROGRAM_ID);
    }

    if (useMetaplex === undefined && !token22) {
      useMetaplex = true;
    }

    const instruction = await this.program.methods
      .logMetadata()
      .accountsStrict({
        authority: this.authority()[0],
        mint,
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: wormholeMessage.publicKey,
          payer: payer || this.provider.publicKey!,
        },
        metadata: useMetaplex ? metadata : null,
        systemProgram: SystemProgram.programId,
        tokenProgram: token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        vault: this.vaultId({mint})[0],
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  async finalizeTransfer({
    destinationNonce,
    transferId,
    mint,
    token,
    amount,
    recipient,
    feeRecipient = null,
    signature,
    payer,
    sequenceNumber,
    token22,
  }: {
    destinationNonce: BN;
    transferId: TransferId;
    mint?: PublicKey;
    token?: string;
    amount: BN;
    recipient: PublicKey;
    feeRecipient?: string | null;
    signature: number[];
    payer?: PublicKey;
    sequenceNumber?: BN;
    token22?: boolean;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();

    if (!mint) {
      if (!token) {
        throw new Error('One of token/mint must be supplied');
      }
      mint = this.wrappedMintId({token})[0];
    }

    const vault = token ? null : this.vaultId({mint})[0];

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    const [config] = this.configId();
    const [usedNonces] = this.usedNoncesId({nonce: destinationNonce});

    if (token22 === undefined) {
      const mintInfo = await this.provider.connection.getAccountInfo(mint);
      token22 = mintInfo?.owner.equals(TOKEN_2022_PROGRAM_ID);
    }

    const tokenAccount = getAssociatedTokenAddressSync(
      mint,
      recipient,
      true,
      token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
    );

    const instruction = await this.program.methods
      .finalizeTransfer({
        payload: {
          destinationNonce,
          transferId,
          amount,
          feeRecipient,
        },
        signature,
      })
      .accountsStrict({
        config,
        usedNonces,
        authority: this.authority()[0],
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: wormholeMessage.publicKey,
          payer: payer || this.provider.publicKey!,
        },
        recipient,
        mint,
        vault,
        systemProgram: SystemProgram.programId,
        tokenProgram: token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenAccount,
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  async initTransfer({
    mint,
    token,
    from,
    user,
    amount,
    recipient,
    fee,
    nativeFee,
    payer,
    sequenceNumber,
    token22,
  }: {
    mint?: PublicKey;
    token?: string;
    from?: PublicKey;
    user?: PublicKey;
    amount: BN;
    recipient: string;
    fee: BN;
    nativeFee: BN;
    payer?: PublicKey;
    sequenceNumber?: BN;
    token22?: boolean;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();

    if (!mint) {
      if (!token) {
        throw new Error('One of token/mint must be supplied');
      }
      mint = this.wrappedMintId({token})[0];
    }

    const vault = token ? null : this.vaultId({mint})[0];

    const [config] = this.configId();

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    if (!user) {
      user = this.provider.publicKey!;
    }

    if (token22 === undefined) {
      const mintInfo = await this.provider.connection.getAccountInfo(mint);
      token22 = mintInfo?.owner.equals(TOKEN_2022_PROGRAM_ID);
    }

    if (!from) {
      from = getAssociatedTokenAddressSync(
        mint,
        user,
        true,
        token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
      );
    }

    const instruction = await this.program.methods
      .initTransfer({
        amount,
        recipient,
        nativeFee,
        fee,
      })
      .accountsStrict({
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: wormholeMessage.publicKey,
          payer: payer || this.provider.publicKey!,
        },
        authority: this.authority()[0],
        solVault: this.solVault()[0],
        mint,
        tokenProgram: token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        from,
        user,
        vault,
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  async initTransferSol({
    user,
    amount,
    recipient,
    nativeFee,
    payer,
    sequenceNumber,
  }: {
    user?: PublicKey;
    amount: BN;
    recipient: string;
    nativeFee: BN;
    payer?: PublicKey;
    sequenceNumber?: BN;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();
    const [config] = this.configId();

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    if (!user) {
      user = this.provider.publicKey!;
    }

    const instruction = await this.program.methods
      .initTransferSol({
        amount,
        recipient,
        fee: new BN(0),
        nativeFee,
      })
      .accountsStrict({
        user,
        authority: this.authority()[0],
        solVault: this.solVault()[0],
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: wormholeMessage.publicKey,
          payer: payer || this.provider.publicKey!,
        },
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  async finalizeTransferSol({
    destinationNonce,
    transferId,
    amount,
    recipient,
    feeRecipient = null,
    signature,
    payer,
    sequenceNumber,
  }: {
    destinationNonce: BN;
    transferId: TransferId;
    amount: BN;
    recipient: PublicKey;
    feeRecipient?: string | null;
    signature: number[];
    payer?: PublicKey;
    sequenceNumber?: BN;
  }): Promise<TransactionData & {message: PublicKey}> {
    const wormholeMessage = Keypair.generate();
    const [config] = this.configId();

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    const instruction = await this.program.methods
      .finalizeTransferSol({
        payload: {
          destinationNonce,
          transferId,
          amount,
          feeRecipient,
        },
        signature,
      })
      .accountsStrict({
        config,
        usedNonces: this.usedNoncesId({nonce: destinationNonce})[0],
        authority: this.authority()[0],
        solVault: this.solVault()[0],
        wormhole: {
          config,
          bridge: this.wormholeBridgeId()[0],
          feeCollector: this.wormholeFeeCollectorId()[0],
          sequence: this.wormholeSequenceId()[0],
          clock: SYSVAR_CLOCK_PUBKEY,
          rent: SYSVAR_RENT_PUBKEY,
          systemProgram: SystemProgram.programId,
          wormholeProgram: this.wormholeProgramId,
          message: wormholeMessage.publicKey,
          payer: payer || this.provider.publicKey!,
        },
        recipient,
        systemProgram: SystemProgram.programId,
      })
      .instruction();

    return {
      instructions: [instruction],
      signers: [wormholeMessage],
      message: wormholeMessage.publicKey,
    };
  }

  static parseWormholeMessage(message: Buffer) {
    let offset = 0;
    const messageType = message.readInt8(offset);
    offset += 1;
    switch (messageType) {
      case 0: {
        let chainId = message.readInt8(offset);
        offset += 1;
        if (chainId !== 2) {
          throw new Error(`Sender has not solana chain ID: ${chainId}`);
        }

        const sender = new PublicKey(message.subarray(offset, offset + 32));
        offset += 32;

        chainId = message.readInt8(offset);
        offset += 1;
        if (chainId !== 2) {
          throw new Error(`Mint has not solana chain ID: ${chainId}`);
        }

        const mint = new PublicKey(message.subarray(offset, offset + 32));
        offset += 32;

        const nonce = new BN(message.subarray(offset, offset + 8), 'le');
        offset += 8;

        const amount = new BN(message.subarray(offset, offset + 16), 'le');
        offset += 16;

        const fee = new BN(message.subarray(offset, offset + 16), 'le');
        offset += 16;

        const nativeFee = new BN(message.subarray(offset, offset + 16), 'le');
        offset += 16;

        const recipientLength = message.readInt32LE(offset);
        offset += 4;
        const recipient = message.toString(
          'utf-8',
          offset,
          offset + recipientLength,
        );
        offset += recipientLength;

        const messageLength = message.readUInt32LE(offset);
        offset += 4;
        const messageData = message.subarray(offset, offset + messageLength);

        return {
          messageType: 'initTransfer',
          sender,
          mint,
          nonce,
          amount,
          fee,
          nativeFee,
          recipient,
          messageData,
        };
      }
      case 3: {
        // LogMetadata

        const chainId = message.readInt8(offset);
        offset += 1;
        if (chainId !== 2) {
          throw new Error(`Mint has not solana chain ID: ${chainId}`);
        }

        const mint = new PublicKey(message.subarray(offset, offset + 32));
        offset += 32;

        const nameLength = message.readUInt32LE(offset);
        offset += 4;
        const name = message.toString('utf-8', offset, offset + nameLength);
        offset += nameLength;

        const symbolLength = message.readUInt32LE(offset);
        offset += 4;
        const symbol = message.toString('utf-8', offset, offset + symbolLength);
        offset += symbolLength;

        const decimals = message.readInt8(offset);
        offset += 1;

        return {
          messageType: 'logMetadata',
          mint,
          name,
          symbol,
          decimals,
        };
      }
      default:
        throw new Error(`Unknown message type: ${messageType}`);
    }
  }
}
