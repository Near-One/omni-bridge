import {IdlAccounts, Program, Provider} from '@coral-xyz/anchor';

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
    admin,
  }: {
    payer?: PublicKey;
    nearBridge: number[];
    admin?: PublicKey;
  }): Promise<TransactionData> {
    const wormholeMessage = Keypair.generate();
    const instruction = await this.program.methods
      .initialize(admin || this.provider.publicKey!, nearBridge)
      .accountsStrict({
        config: this.configId()[0],
        authority: this.authority()[0],
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
    return {instructions: [instruction], signers: [wormholeMessage]};
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
  }): Promise<TransactionData> {
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

    return {instructions: [instruction], signers: [wormholeMessage]};
  }

  async finalizeTransferBridged({
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
  }): Promise<TransactionData> {
    const wormholeMessage = Keypair.generate();

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    const [config] = this.configId();
    const [usedNonces] = this.usedNoncesId({nonce});
    const [mint] = this.wrappedMintId({token});
    const tokenAccount = getAssociatedTokenAddressSync(mint, recipient, true);

    const instruction = await this.program.methods
      .finalizeTransferBridged({
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
          message: wormholeMessage.publicKey,
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

    return {instructions: [instruction], signers: [wormholeMessage]};
  }

  async initTransferBridged({
    token,
    from,
    user,
    amount,
    recipient,
    fee,
    payer,
    sequenceNumber,
  }: {
    token: string;
    from?: PublicKey;
    user?: PublicKey;
    amount: BN;
    recipient: string;
    fee: BN;
    payer?: PublicKey;
    sequenceNumber?: BN;
  }): Promise<TransactionData> {
    const wormholeMessage = Keypair.generate();
    const [config] = this.configId();
    const [mint] = this.wrappedMintId({token});

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    if (!user) {
      user = this.provider.publicKey!;
    }

    if (!from) {
      from = getAssociatedTokenAddressSync(mint, user, true);
    }

    const instruction = await this.program.methods
      .initTransferBridged({
        amount,
        recipient,
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
        mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        from,
        user,
      })
      .instruction();

    return {instructions: [instruction], signers: [wormholeMessage]};
  }

  async registerMint({
    mint,
    name = '',
    symbol = '',
    sequenceNumber,
    overrideAuthority = null,
    useMetaplex,
    payer,
    token22,
  }: {
    mint: PublicKey;
    name?: string;
    symbol?: string;
    sequenceNumber?: BN;
    overrideAuthority?: PublicKey | null;
    useMetaplex?: boolean;
    payer?: PublicKey;
    token22?: boolean;
  }): Promise<TransactionData> {
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

    if (useMetaplex === undefined && !overrideAuthority && !token22) {
      useMetaplex = true;
    }

    const instruction = await this.program.methods
      .registerMint({name, symbol})
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
        overrideAuthority,
        vault: this.vaultId({mint})[0],
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .instruction();

    return {instructions: [instruction], signers: [wormholeMessage]};
  }

  async finalizeTransferNative({
    nonce,
    mint,
    amount,
    recipient,
    feeRecipient = null,
    signature,
    payer,
    sequenceNumber,
    token22,
  }: {
    nonce: BN;
    mint: PublicKey;
    amount: BN;
    recipient: PublicKey;
    feeRecipient?: string | null;
    signature: number[];
    payer?: PublicKey;
    sequenceNumber?: BN;
    token22?: boolean;
  }): Promise<TransactionData> {
    const wormholeMessage = Keypair.generate();

    if (!sequenceNumber) {
      sequenceNumber = await this.fetchNextSequenceNumber();
    }

    const [config] = this.configId();
    const [usedNonces] = this.usedNoncesId({nonce});

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
      .finalizeTransferNative({
        payload: {
          nonce,
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
        vault: this.vaultId({mint})[0],
        systemProgram: SystemProgram.programId,
        tokenProgram: token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenAccount,
      })
      .instruction();

    return {instructions: [instruction], signers: [wormholeMessage]};
  }

  async initTransferNative({
    mint,
    from,
    user,
    amount,
    recipient,
    fee,
    payer,
    sequenceNumber,
    token22,
  }: {
    mint: PublicKey;
    from?: PublicKey;
    user?: PublicKey;
    amount: BN;
    recipient: string;
    fee: BN;
    payer?: PublicKey;
    sequenceNumber?: BN;
    token22?: boolean;
  }): Promise<TransactionData> {
    const wormholeMessage = Keypair.generate();
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
      .initTransferNative({
        amount,
        recipient,
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
        mint,
        tokenProgram: token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        from,
        user,
        vault: this.vaultId({mint})[0],
      })
      .instruction();

    return {instructions: [instruction], signers: [wormholeMessage]};
  }
}
