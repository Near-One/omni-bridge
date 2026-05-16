import { type NetworkProvider, compile } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge } from '../wrappers/OmniBridge';
import { parseArgs } from './_argv';

// Admin op: overwrite the locker's stored `jettonMasterCode` + `jettonWalletCode`
// with the freshly-compiled artifacts. Existing deployed masters/wallets are
// untouched (their code is immutable); future deploy_token calls will use the
// new code.
//
//   bunx blueprint run setJettonCode --testnet --mnemonic

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const value = parsed.value ? BigInt(parsed.value) : toNano('0.05');

    const masterCode = await compile('BridgeJettonMaster');
    const walletCode = await compile('BridgeJettonWallet');

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendSetJettonCode(provider.sender(), {
        value,
        newJettonMasterCode: masterCode,
        newJettonWalletCode: walletCode,
    });

    console.log('set_jetton_code sent');
    console.log('  master code hash =', masterCode.hash().toString('hex'));
    console.log('  wallet code hash =', walletCode.hash().toString('hex'));
    console.log('  attached         =', value.toString(), 'nanoTON');
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
