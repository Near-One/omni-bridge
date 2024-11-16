import {AnchorProvider, Wallet} from '@coral-xyz/anchor';
import {Connection, Keypair, PublicKey} from '@solana/web3.js';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';

const provider = new AnchorProvider(
  new Connection('http://localhost:8899'),
  new Wallet(Keypair.generate()),
  {},
);

const sdk = new OmniBridgeSolanaSDK({
  provider,
  wormholeProgramId: new PublicKey(
    'worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth',
  ),
});
