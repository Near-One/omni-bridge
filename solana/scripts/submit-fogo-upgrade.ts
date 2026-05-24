/**
 * Submit a Squads V3 multisig transaction on Fogo mainnet that upgrades the
 * bridge_token_factory program from a pre-staged buffer.
 *
 * Prereq: a buffer account exists on Fogo containing the new .so AND its
 * buffer authority has been transferred to the multisig vault.
 * (The Makefile target `solana-upgrade-fogo` handles those two steps.)
 *
 * Usage (from solana/scripts):
 *   # Just derive and print the vault PDA (used by the Makefile):
 *   FOGO_MULTISIG=<multisig-pda> npm run --silent upgrade-fogo -- --print-vault
 *
 *   # Dry-run the multisig tx submission:
 *   FOGO_RPC_URL=https://mainnet.fogo.io \
 *   KEYPAIR=$HOME/.config/solana/id.json \
 *   FOGO_MULTISIG=<multisig-pda> \
 *   BUFFER=<buffer-address> \
 *   npm run upgrade-fogo
 *
 *   # Real submission:
 *   ... npm run upgrade-fogo -- --confirm
 */

import * as anchor from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SYSVAR_CLOCK_PUBKEY,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import Squads, {
  DEFAULT_MULTISIG_PROGRAM_ID,
  getAuthorityPDA,
} from "@sqds/sdk";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

const BPF_LOADER_UPGRADEABLE = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);
const BRIDGE_PROGRAM = new PublicKey(
  "dahPEoZGXfyV58JqqH85okdHmpN8U2q8owgPUXSCPxe"
);
const BRIDGE_PROGRAM_DATA = new PublicKey(
  "46ne7xLaLgUwqBTGyezWcgyrvzpXfTPZZkkKR56d5Rhe"
);
const VAULT_AUTHORITY_INDEX = 1;

function loadKeypair(p: string): Keypair {
  return Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf-8")))
  );
}

function buildUpgradeIx(
  buffer: PublicKey,
  spill: PublicKey,
  authority: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: BPF_LOADER_UPGRADEABLE,
    keys: [
      { pubkey: BRIDGE_PROGRAM_DATA, isWritable: true, isSigner: false },
      { pubkey: BRIDGE_PROGRAM, isWritable: true, isSigner: false },
      { pubkey: buffer, isWritable: true, isSigner: false },
      { pubkey: spill, isWritable: true, isSigner: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isWritable: false, isSigner: false },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isWritable: false, isSigner: false },
      { pubkey: authority, isWritable: false, isSigner: true },
    ],
    data: Buffer.from([3, 0, 0, 0]),
  });
}

async function main() {
  const multisigStr = process.env.FOGO_MULTISIG;
  if (!multisigStr) {
    console.error("FOGO_MULTISIG env var is required (the multisig PDA)");
    process.exit(1);
  }
  const multisig = new PublicKey(multisigStr);

  const [vault] = await getAuthorityPDA(
    multisig,
    new anchor.BN(VAULT_AUTHORITY_INDEX),
    DEFAULT_MULTISIG_PROGRAM_ID
  );

  if (process.argv.includes("--print-vault")) {
    process.stdout.write(vault.toBase58());
    return;
  }

  const bufferStr = process.env.BUFFER;
  if (!bufferStr) {
    console.error(
      "BUFFER env var is required (the buffer address from `solana program write-buffer`)"
    );
    process.exit(1);
  }
  const buffer = new PublicKey(bufferStr);

  const rpcUrl = process.env.FOGO_RPC_URL ?? "https://mainnet.fogo.io";
  const keypairPath =
    process.env.KEYPAIR ?? path.join(os.homedir(), ".config/solana/id.json");
  const confirm = process.argv.includes("--confirm");
  const printMessage = process.argv.includes("--print-message");

  const connection = new Connection(rpcUrl, "confirmed");
  const wallet = new anchor.Wallet(loadKeypair(keypairPath));
  // Where the buffer's reclaimed rent goes after the upgrade. Defaults to the
  // submitter, but can be overridden (e.g., refund the wallet that originally
  // paid for the buffer when a different member generates the message).
  const spill = process.env.SPILL
    ? new PublicKey(process.env.SPILL)
    : wallet.publicKey;

  console.log("=== Squads V3 Program Upgrade — Fogo mainnet ===");
  console.log("RPC:              ", rpcUrl);
  console.log("Multisig:         ", multisig.toBase58());
  console.log("Vault (signer):   ", vault.toBase58());
  console.log("Bridge program:   ", BRIDGE_PROGRAM.toBase58());
  console.log("ProgramData:      ", BRIDGE_PROGRAM_DATA.toBase58());
  console.log("Buffer:           ", buffer.toBase58());
  console.log("Spill (recipient):", spill.toBase58());
  console.log("Submitter:        ", wallet.publicKey.toBase58());

  const bufferInfo = await connection.getAccountInfo(buffer);
  if (!bufferInfo) {
    console.error("\nBuffer account not found on-chain. Aborting.");
    process.exit(1);
  }
  if (!bufferInfo.owner.equals(BPF_LOADER_UPGRADEABLE)) {
    console.error(
      `\nBuffer is not owned by BPF Loader Upgradeable (owner: ${bufferInfo.owner.toBase58()}). Aborting.`
    );
    process.exit(1);
  }

  const upgradeIx = buildUpgradeIx(buffer, spill, vault);

  // --print-message: build a Solana legacy Message containing just the upgrade
  // instruction, base58-encode it, and print. squads-cli's "Enter Transaction
  // (base58 serialized message)" option accepts exactly this format and wraps
  // it into a multisig tx — usable from any member's machine (Ledger or not).
  if (printMessage) {
    const tx = new Transaction();
    tx.feePayer = vault; // squads-cli ignores feePayer & blockhash; dummy is fine
    tx.recentBlockhash = "11111111111111111111111111111111";
    tx.add(upgradeIx);
    const msgBase58 = anchor.utils.bytes.bs58.encode(tx.serializeMessage());
    console.log(
      '\n--- Paste into squads-cli "Enter Transaction (base58 serialized message)": ---\n'
    );
    console.log(msgBase58);
    return;
  }

  if (!confirm) {
    console.log(
      "\n[DRY RUN] No multisig transaction created. Re-run with --confirm to submit."
    );
    return;
  }

  const squads = Squads.endpoint(rpcUrl, wallet);

  console.log("\nCreating multisig transaction...");
  const tx = await squads.createTransaction(multisig, VAULT_AUTHORITY_INDEX);
  console.log("Transaction PDA:  ", tx.publicKey.toBase58());

  console.log("Adding upgrade instruction...");
  await squads.addInstruction(tx.publicKey, upgradeIx);

  console.log("Activating transaction...");
  await squads.activateTransaction(tx.publicKey);

  console.log("\n✓ Multisig transaction ready for voting:");
  console.log("  ", tx.publicKey.toBase58());
  console.log("\nNext steps:");
  console.log(
    `  1) Members vote (need ${"threshold"} approvals): squads-cli --cluster ${rpcUrl}`
  );
  console.log(`     → Multisig ${multisig.toBase58()} → Transactions → ${tx.publicKey.toBase58()}`);
  console.log(`  2) Once threshold reached, any member executes — that triggers the on-chain upgrade.`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
