import type { NetworkProvider } from '@ton/blueprint';
import { Address, beginCell, toNano } from '@ton/core';
import { Opcodes, bytesToCell } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// User → locker TEP-74 jetton transfer with bridge-tagged forward_payload.
// Triggers the locker's `transfer_notification` handler → InitTransferEvent.
//
//   bunx blueprint run transferJetton --testnet --mnemonic -- \
//       --jettonWallet EQ<user-jetton-wallet> \
//       --amount 10000000 \
//       --recipient near:alice.testnet
//
// Get the user's jetton wallet address from the master's `get_wallet_address`
// get-method (e.g. on tonviewer), or compute deterministically if you know the
// wallet code.
//
//   Optional:
//       --fee <jetton-subunits>        default 0
//       --nativeFee <nanoTON>           default 0
//       --message <utf8>                default empty
//       --queryId <uint64>              default 0
//       --forwardTon <nanoTON>          forward_ton_amount in the TEP-74 header;
//                                       must be > the locker's compute cost
//                                       (default 0.15 TON)

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const jettonWallet = Address.parse(mustArg(parsed, 'jettonWallet'));
    const amount = BigInt(mustArg(parsed, 'amount'));
    const recipient = mustArg(parsed, 'recipient');
    const fee = BigInt(parsed.fee ?? '0');
    const nativeFee = BigInt(parsed.nativeFee ?? '0');
    const message = parsed.message ?? '';
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;
    const forwardTon = parsed.forwardTon ? BigInt(parsed.forwardTon) : toNano('0.15');

    const lockerAddr = Address.parse(mustLockerEnv());
    const sender = provider.sender();
    if (!sender.address) throw new Error('sender has no address');

    // forward_payload content the locker will parse
    const forward = beginCell()
        .storeUint(Opcodes.INIT_TRANSFER_JETTON_FWD, 32)
        .storeUint(fee, 128)
        .storeUint(nativeFee, 128)
        .storeRef(bytesToCell(Buffer.from(recipient, 'utf8')))
        .storeRef(bytesToCell(Buffer.from(message, 'utf8')))
        .endCell();

    // Standard TEP-74 transfer
    const body = beginCell()
        .storeUint(Opcodes.TEP74_TRANSFER, 32)
        .storeUint(queryId, 64)
        .storeCoins(amount)
        .storeAddress(lockerAddr) // destination
        .storeAddress(sender.address) // response_destination
        .storeUint(0, 1) // custom_payload = null
        .storeCoins(forwardTon)
        .storeUint(1, 1) // forward_payload flag = ref
        .storeRef(forward)
        .endCell();

    await sender.send({
        to: jettonWallet,
        value: forwardTon + toNano('0.1'), // cover JW compute + forward_ton
        body,
    });

    console.log('TEP-74 transfer sent to user jetton wallet');
    console.log('  jw         =', jettonWallet.toString({ testOnly: true }));
    console.log('  amount     =', amount.toString());
    console.log('  recipient  =', recipient);
    console.log('  forwardTon =', forwardTon.toString(), 'nanoTON');
    console.log();
    console.log(
        `Watch locker ext-out at https://testnet.tonviewer.com/${lockerAddr.toString({ testOnly: true })}`,
    );
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
