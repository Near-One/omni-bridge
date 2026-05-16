import type { NetworkProvider } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { JettonKind, OmniBridge } from '../wrappers/OmniBridge';
import { mustArg, parseArgs } from './_argv';

// Admin-only fallback: register a jetton in the locker without going through
// the permissionless TEP-89 handshake. Use when a master doesn't implement
// `provide_wallet_address` or when you want to bootstrap registration before
// the TEP-89 reply lands.
//
//   bunx blueprint run registerJetton --testnet --mnemonic -- \
//       --master EQ<USDT_MASTER> \
//       --lockerJw EQ<locker-usdt-wallet> \
//       --kind LOCKED_NATIVE \
//       --decimals 6
//
// lockerJw: compute off-chain via the master's `get_wallet_address(locker)`
// get-method. Paste the result.
//
// kind: LOCKED_NATIVE (external TEP-74) | BRIDGE_MINTED (master we own);
//       BRIDGE_MINTED here is mostly a testing convenience — in prod,
//       bridge-minted jettons come from `deploy_token`, not admin registration.
//
// Caller must be the locker admin.

export async function run(provider: NetworkProvider, args: string[]) {
    const parsed = parseArgs(args);
    const master = Address.parse(mustArg(parsed, 'master'));
    const lockerJw = Address.parse(mustArg(parsed, 'lockerJw'));
    const kindStr = mustArg(parsed, 'kind');
    const decimals = Number(mustArg(parsed, 'decimals'));
    const queryId = parsed.queryId ? BigInt(parsed.queryId) : 0n;

    const kind =
        kindStr === 'LOCKED_NATIVE'
            ? JettonKind.LOCKED_NATIVE
            : kindStr === 'BRIDGE_MINTED'
              ? JettonKind.BRIDGE_MINTED
              : (() => {
                    throw new Error(`kind must be LOCKED_NATIVE or BRIDGE_MINTED, got ${kindStr}`);
                })();

    const lockerAddr = Address.parse(mustLockerEnv());
    const bridge = provider.open(OmniBridge.createFromAddress(lockerAddr));

    await bridge.sendRegisterJetton(provider.sender(), {
        value: toNano('0.05'),
        queryId,
        kind,
        master,
        lockerJw,
        decimals,
    });

    console.log('register_jetton sent (admin-only)');
    console.log('  master    =', master.toString({ testOnly: true }));
    console.log('  lockerJw  =', lockerJw.toString({ testOnly: true }));
    console.log('  kind      =', kindStr);
    console.log('  decimals  =', decimals);
}

function mustLockerEnv(): string {
    const v = process.env.TON_LOCKER;
    if (!v) throw new Error('TON_LOCKER not set');
    return v;
}
