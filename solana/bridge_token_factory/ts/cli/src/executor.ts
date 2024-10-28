import {
  AddressLookupTableProgram,
  Signer,
  Transaction,
  TransactionInstruction,
  TransactionMessage,
} from '@solana/web3.js';
import {getContext} from './context';
import {PublicKey} from '@solana/web3.js';
import {VersionedTransaction} from '@solana/web3.js';
import BN from 'bn.js';
import {TOKEN_PROGRAM_ID} from '@solana/spl-token';

const MAX_TX_KEYS = 20;

async function findLookupTable(): Promise<PublicKey | undefined> {
  // eslint-disable-next-line prefer-const
  let {provider} = getContext();

  const lookupTables = await provider.connection.getParsedProgramAccounts(
    AddressLookupTableProgram.programId,
    {
      filters: [
        {
          memcmp: {
            offset: 22,
            bytes: provider.publicKey!.toBase58(),
          },
        },
      ],
    },
  );
  // console.log(YAML.stringify(lookupTables));
  let theLatestTable;
  for (const table of lookupTables) {
    const deactivationSlot = new BN(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (table.account.data as any).parsed.info.deactivationSlot,
    );
    if (deactivationSlot.eq(new BN('18446744073709551615'))) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const lastExtendedSlot = new BN(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (table.account.data as any).parsed.info.lastExtendedSlot,
      );
      const oldLastExtendedSlot = new BN(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (theLatestTable?.account.data as any)?.parsed.info.lastExtendedSlot,
      );
      if (!oldLastExtendedSlot || lastExtendedSlot.gt(oldLastExtendedSlot)) {
        theLatestTable = table;
      }
    }
  }

  for (const table of lookupTables) {
    const slot = new BN(await provider.connection.getSlot());
    const deactivationSlot = new BN(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (table.account.data as any).parsed.info.deactivationSlot,
    );
    if (deactivationSlot.addn(513).lt(slot)) {
      const tx = new Transaction();
      tx.add(
        AddressLookupTableProgram.closeLookupTable({
          lookupTable: table.pubkey,
          authority: provider.publicKey!,
          recipient: provider.publicKey!,
        }),
      );
      const r = await provider.sendAndConfirm!(tx);
      console.log(`Closed lookup table ${table.pubkey.toBase58()}: ${r}`);
    } else if (
      deactivationSlot.eq(new BN('18446744073709551615')) &&
      table !== theLatestTable
    ) {
      const tx = new Transaction();
      tx.add(
        AddressLookupTableProgram.deactivateLookupTable({
          lookupTable: table.pubkey,
          authority: provider.publicKey!,
        }),
      );
      const r = await provider.sendAndConfirm!(tx);
      console.log(`Deactivated lookup table ${table.pubkey.toBase58()}: ${r}`);
    }
  }

  return theLatestTable?.pubkey;
}

export async function executeTx({
  instructions,
  signers,
  lowPriorityAccounts = [],
}: {
  instructions: TransactionInstruction[];
  signers?: Signer[];
  lowPriorityAccounts?: PublicKey[];
}) {
  let {
    // eslint-disable-next-line prefer-const
    provider,
    // eslint-disable-next-line prefer-const
    simulate,
    lookupTable,
  } = getContext();

  const addresses = new Set<string>();
  for (const instruction of instructions) {
    for (const {pubkey} of instruction.keys) {
      addresses.add(pubkey.toBase58());
    }
  }

  // Remove top level programs
  addresses.delete(provider.publicKey!.toBase58());
  addresses.delete(TOKEN_PROGRAM_ID.toBase58());

  if (addresses.size > 256) {
    throw new Error('Too many accounts');
  }

  let missingAddresses = new Set(addresses);

  if (!lookupTable && addresses.size > MAX_TX_KEYS) {
    lookupTable = await findLookupTable();
    if (lookupTable) {
      const lookupTableAccount = (
        await provider.connection.getAddressLookupTable(lookupTable)
      ).value!;
      for (const a of lookupTableAccount.state.addresses) {
        missingAddresses.delete(a.toBase58());
      }
      if (
        missingAddresses.size + lookupTableAccount.state.addresses.length >
        256
      ) {
        const tx = new Transaction();
        tx.add(
          AddressLookupTableProgram.deactivateLookupTable({
            lookupTable: lookupTable,
            authority: provider.publicKey!,
          }),
        );
        const r = await provider.sendAndConfirm!(tx);
        console.log(`Deactivated lookup table ${lookupTable.toBase58()}: ${r}`);
        lookupTable = undefined;
        missingAddresses = new Set(addresses);
      }
    }
    if (!lookupTable) {
      const slot = await provider.connection.getSlot();

      const [lookupTableInst, lookupTableAddress] =
        AddressLookupTableProgram.createLookupTable({
          authority: provider.publicKey!,
          payer: provider.publicKey!,
          recentSlot: slot,
        });
      lookupTable = lookupTableAddress;

      const tx = new Transaction();
      tx.add(lookupTableInst);
      const r = await provider.sendAndConfirm!(tx);
      console.log(`Created lookup table ${lookupTableAddress}: ${r}`);
    }
  }

  let lookupTableAccount;
  if (lookupTable) {
    const lowPriority = new Set(lowPriorityAccounts.map(a => a.toBase58()));
    const addressList = [];
    for (const address of missingAddresses.values()) {
      if (lowPriority.has(address)) {
        continue;
      }
      addressList.push(new PublicKey(address));
    }
    for (const address of lowPriority) {
      if (missingAddresses.has(address)) {
        addressList.push(new PublicKey(address));
      }
    }
    let r;
    while (addressList.length > MAX_TX_KEYS) {
      const chunk = addressList.splice(0, 16);
      const tx = new Transaction();
      tx.add(
        AddressLookupTableProgram.extendLookupTable({
          payer: provider.publicKey!,
          authority: provider.publicKey!,
          lookupTable: lookupTable,
          addresses: chunk,
        }),
      );
      r = await provider.sendAndConfirm!(tx);
      console.log(`Added accounts to lookup table ${r}`);
    }
    if (r !== undefined) {
      await new Promise(resolve => setTimeout(resolve, 2000));
    }

    lookupTableAccount = (
      await provider.connection.getAddressLookupTable(lookupTable)
    ).value!;
  }

  const {blockhash} = await provider.connection.getLatestBlockhash();
  const message = new TransactionMessage({
    payerKey: provider.publicKey!,
    recentBlockhash: blockhash,
    instructions,
  }).compileToV0Message(lookupTableAccount ? [lookupTableAccount] : []);

  const tx = new VersionedTransaction(message);

  if (simulate) {
    const r = await provider.simulate!(tx, signers);
    console.log('Simulation:');
    for (const line of r.logs || []) {
      console.log(line);
    }
  } else {
    const r = await provider.sendAndConfirm!(tx, signers);
    console.log(`Tx hash: ${r}`);
  }
}
