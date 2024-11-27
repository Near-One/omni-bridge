import {Command} from 'commander';
import {getContext} from './context';
import {executeTx} from './executor';
import {
  createInitializeMintInstruction,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
  getMinimumBalanceForRentExemptMint,
  getAssociatedTokenAddressSync,
  createMintToInstruction,
  createAssociatedTokenAccountInstruction,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getMintLen,
  ExtensionType,
  TOKEN_2022_PROGRAM_ID,
  createInitializeMetadataPointerInstruction,
  TYPE_SIZE,
  LENGTH_SIZE,
  createInitializeInstruction,
  createInitializeMint2Instruction,
} from '@solana/spl-token';
import {
  Keypair,
  PublicKey,
  Struct,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from '@solana/web3.js';
import {parseKeypair} from './keyParser';
import {struct, u8, u16, str, option} from '@coral-xyz/borsh';

const METADATA_PROGRAM_ID = new PublicKey(
  'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s',
);

class CreateMetadataArgs {
  instruction = 0;
  name: string;
  symbol: string;
  uri: string;
  sellerFeeBasisPoints: number;
  creators: null;

  constructor(args: {
    name: string;
    symbol: string;
    uri: string;
    sellerFeeBasisPoints: number;
  }) {
    this.name = args.name;
    this.symbol = args.symbol;
    this.uri = args.uri;
    this.sellerFeeBasisPoints = args.sellerFeeBasisPoints;
    this.creators = null;
  }
}

const METADATA_SCHEMA = struct([
  u8('instruction'),
  str('name'),
  str('symbol'),
  str('uri'),
  u16('sellerFeeBasisPoints'),
  option(u8(), 'creators'),
]);

export function installCreateTokenCLI(program: Command) {
  program
    .command('create-token')
    .description('Deploy the token')
    .option('--mint <keypair>', 'Mint address')
    .requiredOption('--name <string>', 'Token name')
    .requiredOption('--symbol <string>', 'Token symbol')
    .requiredOption('--decimals <number>', 'Token decimals', parseInt)
    .option('--token22', 'use token22 standard')
    .action(
      async ({
        mint,
        name,
        symbol,
        decimals,
        token22,
      }: {
        mint?: string;
        name: string;
        symbol: string;
        decimals: number;
        token22?: boolean;
      }) => {
        const {sdk} = getContext();

        const mintKp = mint ? await parseKeypair(mint) : Keypair.generate();
        console.log(`Creating mint ${mintKp.publicKey.toBase58()}`);
        const tokenAccount = getAssociatedTokenAddressSync(
          mintKp.publicKey,
          sdk.provider.publicKey!,
          false,
          token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
        );

        let ixes: TransactionInstruction[] = [];

        if (token22) {
          const mintLen =
            getMintLen([ExtensionType.MetadataPointer]) +
            TYPE_SIZE +
            LENGTH_SIZE +
            500;
          ixes = [
            SystemProgram.createAccount({
              fromPubkey: sdk.provider.publicKey!,
              newAccountPubkey: mintKp.publicKey,
              space: getMintLen([ExtensionType.MetadataPointer]),
              lamports:
                await sdk.provider.connection.getMinimumBalanceForRentExemption(
                  mintLen,
                ),
              programId: TOKEN_2022_PROGRAM_ID,
            }),
            createInitializeMetadataPointerInstruction(
              mintKp.publicKey,
              sdk.provider.publicKey!,
              mintKp.publicKey,
              TOKEN_2022_PROGRAM_ID,
            ),
            createInitializeMintInstruction(
              mintKp.publicKey,
              decimals,
              sdk.provider.publicKey!,
              null,
              TOKEN_2022_PROGRAM_ID,
            ),
            createInitializeInstruction({
              programId: TOKEN_2022_PROGRAM_ID,
              metadata: mintKp.publicKey,
              updateAuthority: sdk.provider.publicKey!,
              mint: mintKp.publicKey,
              mintAuthority: sdk.provider.publicKey!,
              name,
              symbol,
              uri: '',
            }),
          ];
        } else {
          const [metadataAccount] = await PublicKey.findProgramAddress(
            [
              Buffer.from('metadata'),
              METADATA_PROGRAM_ID.toBuffer(),
              mintKp.publicKey.toBuffer(),
            ],
            METADATA_PROGRAM_ID,
          );

          const metadataData = {
            instruction: 0,
            name,
            symbol,
            uri: '',
            sellerFeeBasisPoints: 0,
            creators: null,
          };

          const serializedData = Buffer.alloc(1000);
          METADATA_SCHEMA.encode(metadataData, serializedData);

          ixes = [
            SystemProgram.createAccount({
              fromPubkey: sdk.provider.publicKey!,
              newAccountPubkey: mintKp.publicKey,
              space: MINT_SIZE,
              lamports: await getMinimumBalanceForRentExemptMint(
                sdk.provider.connection,
              ),
              programId: TOKEN_PROGRAM_ID,
            }),
            createInitializeMintInstruction(
              mintKp.publicKey,
              decimals,
              sdk.provider.publicKey!,
              null,
              TOKEN_PROGRAM_ID,
            ),
            new TransactionInstruction({
              keys: [
                {pubkey: metadataAccount, isSigner: false, isWritable: true},
                {pubkey: mintKp.publicKey, isSigner: false, isWritable: false},
                {
                  pubkey: sdk.provider.publicKey!,
                  isSigner: true,
                  isWritable: false,
                },
                {
                  pubkey: SystemProgram.programId,
                  isSigner: false,
                  isWritable: false,
                },
              ],
              programId: METADATA_PROGRAM_ID,
              data: serializedData.slice(
                0,
                METADATA_SCHEMA.getSpan(serializedData),
              ),
            }),
          ];
        }
        ixes.push(
          createAssociatedTokenAccountInstruction(
            sdk.provider.publicKey!,
            tokenAccount,
            sdk.provider.publicKey!,
            mintKp.publicKey,
            token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID,
          ),
          createMintToInstruction(
            mintKp.publicKey,
            tokenAccount,
            sdk.provider.publicKey!,
            1000 * Math.pow(10, decimals),
            [],
            token22 ? TOKEN_2022_PROGRAM_ID : TOKEN_PROGRAM_ID,
          ),
        );
        await executeTx({instructions: ixes, signers: [mintKp]});
      },
    );
}
