/* eslint-disable n/no-unsupported-features/es-builtins */
import {Keypair, PublicKey, SystemProgram} from '@solana/web3.js';
import BN from 'bn.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {Base64} from 'js-base64';
import {
  AccountLayout,
  AccountState,
  getAssociatedTokenAddressSync,
  MintLayout,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';
import {
  findMetadataPda,
  getMetadataAccountDataSerializer,
  MetadataAccountDataArgs,
  MPL_TOKEN_METADATA_PROGRAM_ID,
} from '@metaplex-foundation/mpl-token-metadata';
import {Umi} from '@metaplex-foundation/umi';

BN.prototype.toJSON = function () {
  return this.toString();
};

const ACCOUNT_STORAGE_OVERHEAD = 128;
const DEFAULT_LAMPORTS_PER_BYTE_YEAR = Math.floor(
  ((1_000_000_000 / 100) * 365) / (1024 * 1024),
);
const DEFAULT_EXEMPTION_THRESHOLD = 2.0;

export function getMinimumBalanceForRentExemption(bytes: number) {
  return (
    (ACCOUNT_STORAGE_OVERHEAD + bytes) *
    DEFAULT_LAMPORTS_PER_BYTE_YEAR *
    DEFAULT_EXEMPTION_THRESHOLD
  );
}

export const programIdKp = Keypair.fromSecretKey(
  Uint8Array.from([
    225, 34, 97, 224, 178, 48, 236, 237, 241, 233, 132, 211, 119, 49, 88, 177,
    166, 27, 217, 184, 217, 106, 155, 103, 153, 230, 150, 210, 195, 72, 9, 57,
    38, 35, 227, 206, 5, 147, 218, 190, 207, 202, 141, 133, 60, 31, 98, 56, 108,
    157, 32, 138, 168, 136, 244, 155, 16, 157, 174, 238, 124, 95, 238, 37,
  ]),
); // 3ZtEZ8xABFbUr4c1FVpXbQiVdqv4vwhvfCc8HMmhEeua

export function systemAccount({
  address,
  balance,
}: {
  address: PublicKey;
  balance: BN;
}) {
  return {
    pubkey: address.toBase58(),
    account: {
      lamports: balance.toNumber(),
      data: ['', 'base64'],
      owner: SystemProgram.programId.toBase58(),
      executable: false,
      rentEpoch: 0,
    },
  };
}

export async function omniBridgeAccount<T>({
  sdk,
  account,
  accountType,
}: {
  sdk: OmniBridgeSolanaSDK;
  account: T;
  accountType: string;
}) {
  const data = await sdk.program.coder.accounts.encode(accountType, account);
  return {
    pubkey: sdk.configId()[0].toBase58(),
    account: {
      lamports: getMinimumBalanceForRentExemption(data.length),
      data: [Base64.fromUint8Array(data), 'base64'],
      owner: sdk.programId.toBase58(),
      executable: false,
      rentEpoch: 0,
    },
  };
}

export type MintAccountArgs = {
  mint: PublicKey;
  decimals: number;
  supply: BN;
  mintAuthority?: PublicKey;
  freezeAuthority?: PublicKey;
};

export function mintAccount({
  mint,
  decimals,
  supply,
  mintAuthority,
  freezeAuthority,
}: MintAccountArgs) {
  const data = Buffer.alloc(MintLayout.span);

  MintLayout.encode(
    {
      mintAuthorityOption: mintAuthority ? 1 : 0,
      mintAuthority: mintAuthority || PublicKey.default,
      supply: BigInt(supply.toString()),
      decimals,
      isInitialized: true,
      freezeAuthorityOption: freezeAuthority ? 1 : 0,
      freezeAuthority: freezeAuthority || PublicKey.default,
    },
    data,
  );

  return {
    pubkey: mint.toBase58(),
    account: {
      lamports: getMinimumBalanceForRentExemption(data.length),
      data: [Base64.fromUint8Array(data), 'base64'],
      owner: TOKEN_PROGRAM_ID.toBase58(),
      executable: false,
      rentEpoch: 0,
    },
  };
}

export function metadataAccount({
  umi,
  ...metadata
}: MetadataAccountDataArgs & {
  umi: Umi;
}) {
  return {
    pubkey: findMetadataPda(umi, {mint: metadata.mint})[0],
    account: {
      lamports: getMinimumBalanceForRentExemption(0),
      data: [
        Base64.fromUint8Array(
          getMetadataAccountDataSerializer().serialize(metadata),
        ),
        'base64',
      ],
      owner: MPL_TOKEN_METADATA_PROGRAM_ID,
      executable: false,
      rentEpoch: 0,
    },
  };
}

export type TokenAccountArgs = {
  address?: PublicKey;
  mint: PublicKey;
  owner: PublicKey;
  amount: BN;
  delegate?: PublicKey | null;
  isFrozen?: boolean;
  native?: BN;
  delegatedAmount?: BN;
  closeAuthority?: PublicKey | null;
};

export function tokenAccount({
  address,
  mint,
  owner,
  amount,
  delegate,
  isFrozen = false,
  native,
  delegatedAmount = new BN(0),
  closeAuthority,
}: TokenAccountArgs) {
  if (!address) {
    address = getAssociatedTokenAddressSync(mint, owner, true);
  }
  const data = Buffer.alloc(AccountLayout.span);
  AccountLayout.encode(
    {
      mint,
      owner,
      amount: BigInt(amount.toString()),
      delegateOption: delegate ? 1 : 0,
      delegate: delegate || PublicKey.default,
      state: isFrozen ? AccountState.Frozen : AccountState.Initialized,
      isNativeOption: native ? 1 : 0,
      isNative: BigInt(native?.toString() || 0),
      delegatedAmount: BigInt(delegatedAmount.toString()),
      closeAuthorityOption: closeAuthority ? 1 : 0,
      closeAuthority: closeAuthority || PublicKey.default,
    },
    data,
  );

  return {
    pubkey: address.toBase58(),
    account: {
      lamports: getMinimumBalanceForRentExemption(data.length),
      data: [Base64.fromUint8Array(data), 'base64'],
      owner: TOKEN_PROGRAM_ID.toBase58(),
      executable: false,
      rentEpoch: 0,
    },
  };
}
