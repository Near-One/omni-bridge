import {AnchorProvider, Provider, Wallet} from '@coral-xyz/anchor';
import expandTilde from 'expand-tilde';
import * as YAML from 'yaml';
import {fs} from 'mz';
import {
  Cluster,
  Commitment,
  Connection,
  PublicKey,
  clusterApiUrl,
} from '@solana/web3.js';
import {parseKeypair} from './keyParser';
import {OmniBridgeSolanaSDK} from 'omni-bridge-solana-sdk';

export type PrintFormat = 'multisig' | 'legacy' | '0';

export type Context = {
  provider: Provider;
  keyMap: Map<string, PublicKey>;
  network: Cluster;
  sdk: OmniBridgeSolanaSDK;
  lookupTable?: PublicKey;
  simulate: boolean;
  print: PrintFormat;
};

const context = {
  ref: undefined! as Context,
};

export function setContext(ctx: Context) {
  context.ref = ctx;
}

export function getContext() {
  return context.ref;
}

export async function setupContext({
  keyMap,
  config,
  cluster,
  wallet,
  commitment,
  skipPreflight,
  lookupTable,
  simulate = false,
  print,
}: {
  keyMap?: string;
  config?: string;
  cluster?: string;
  wallet?: string;
  commitment?: string;
  skipPreflight?: boolean;
  lookupTable?: string;
  simulate?: boolean;
  print?: string;
}) {
  if (context.ref) {
    return; // For unit testing purpose
  }

  const keyMapData = new Map<string, PublicKey>();
  if (keyMap) {
    try {
      const keys = JSON.parse(await fs.readFile(keyMap, 'utf-8'));
      for (const [name, key] of Object.entries(keys)) {
        keyMapData.set(name, new PublicKey(key as string));
      }
    } catch (err) {
      console.error('Failed to read key map', err);
    }
  }

  if (!cluster && !wallet && !commitment && !config) {
    config = '~/.config/solana/cli/config.yml';
  }

  if (config) {
    config = expandTilde(config);
    const configData: {
      json_rpc_url: string;
      // websocket_url: string;
      keypair_path: string;
      commitment: string;
    } = YAML.parse(await fs.readFile(config, 'utf-8'));
    if (!cluster) {
      cluster = configData.json_rpc_url;
    }
    if (!wallet) {
      wallet = configData.keypair_path;
    }
    if (!commitment) {
      commitment = configData.commitment;
    }
  } else {
    if (!cluster) {
      cluster = 'http://localhost:8899';
    }
    if (!wallet) {
      wallet = '~/.config/solana/id.json';
    }
  }

  let network: Cluster = 'mainnet-beta';

  if (cluster in ['devnet', 'testnet', 'mainnet-beta']) {
    network = cluster as Cluster;
    cluster = clusterApiUrl(network);
  } else if (cluster.indexOf('dev') !== -1) {
    // Url to cluster hack
    network = 'devnet';
  }

  const connection = new Connection(cluster, {
    commitment: commitment as Commitment | undefined,
  });

  const walletKp = await parseKeypair(wallet);
  const lookupTablePk = lookupTable ? new PublicKey(lookupTable) : undefined;

  const provider = new AnchorProvider(connection, new Wallet(walletKp), {
    skipPreflight,
    commitment: commitment as Commitment | undefined,
  });

  const sdk = new OmniBridgeSolanaSDK({
    provider,
    wormholeProgramId:
      network === 'devnet'
        ? new PublicKey('3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5')
        : new PublicKey('worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth'),
  });

  setContext({
    provider,
    keyMap: keyMapData,
    network,
    sdk,
    lookupTable: lookupTablePk,
    simulate,
    print: print as PrintFormat,
  });
}
