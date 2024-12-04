import {AnchorProvider, Wallet} from '@coral-xyz/anchor';
import {Connection, Keypair, PublicKey} from '@solana/web3.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';
import {setup} from './data';
import {createUmi} from '@metaplex-foundation/umi-bundle-defaults';

const provider = new AnchorProvider(
  new Connection('http://localhost:8899'),
  new Wallet(Keypair.generate()),
  {},
);

const umi = createUmi(provider.connection);

const sdk = new OmniBridgeSolanaSDK({
  provider,
  wormholeProgramId: new PublicKey(
    'worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth',
  ),
});

setup({sdk, umi}).catch(e => {
  throw e;
});
