import { type NetworkProvider, compile } from '@ton/blueprint';
import { Address, toNano } from '@ton/core';
import { OmniBridge, TON_CHAIN_ID } from '../wrappers/OmniBridge';

// Deploys the three compiled contracts and the OmniBridge locker with a
// pre-derived mock MPC signer address.
//
// Prerequisites (.env):
//   WALLET_MNEMONIC="24 words..."
//   WALLET_VERSION=v5r1
//   WALLET_NETWORK=testnet
//   MPC_DERIVED_ADDR=0x...    (from `npx blueprint run deriveMpc`)
//   TON_ADMIN=EQ...           (optional; defaults to the deployer's own wallet)
//
// Usage:
//   npx blueprint run deployOmniBridge --testnet --mnemonic

export async function run(provider: NetworkProvider) {
    const mpcHex = process.env.MPC_DERIVED_ADDR;
    if (!mpcHex) {
        throw new Error('Set MPC_DERIVED_ADDR in .env. Run `npx blueprint run deriveMpc` first.');
    }
    const nearBridgeDerivedAddr = BigInt(mpcHex);

    const senderAddr = provider.sender().address;
    if (!senderAddr) throw new Error('Deployer has no address');

    const admin = process.env.TON_ADMIN ? Address.parse(process.env.TON_ADMIN) : senderAddr;

    console.log('Compiling contracts…');
    const bridgeCode = await compile('OmniBridge');
    const masterCode = await compile('BridgeJettonMaster');
    const walletCode = await compile('BridgeJettonWallet');

    const bridge = provider.open(
        OmniBridge.createFromConfig(
            {
                admin,
                nearBridgeDerivedAddr,
                chainId: TON_CHAIN_ID,
                jettonMasterCode: masterCode,
                jettonWalletCode: walletCode,
            },
            bridgeCode,
        ),
    );

    const userFriendly = bridge.address.toString({ testOnly: true, bounceable: true });
    const raw = bridge.address.toRawString();

    console.log();
    console.log('Config:');
    console.log('  admin                =', admin.toString({ testOnly: true }));
    console.log(
        '  nearBridgeDerivedAddr=',
        `0x${nearBridgeDerivedAddr.toString(16).padStart(40, '0')}`,
    );
    console.log('  chainId              =', TON_CHAIN_ID);
    console.log();
    console.log('Pre-deploy address:');
    console.log('  user-friendly =', userFriendly);
    console.log('  raw           =', raw);
    console.log();

    if (await provider.isContractDeployed(bridge.address)) {
        console.log('Already deployed — exiting.');
        return;
    }

    console.log('Sending deploy message (3 TON for storage headroom)…');
    await bridge.sendDeploy(provider.sender(), toNano('3'));

    await provider.waitForDeploy(bridge.address, 30);

    const state = await bridge.getState();
    console.log();
    console.log('✓ Deployed!');
    console.log('  user-friendly:', userFriendly);
    console.log('  raw:          ', raw);
    console.log();
    console.log('Explorers:');
    console.log(`  https://testnet.tonviewer.com/${userFriendly}`);
    console.log(`  https://testnet.tonscan.org/address/${userFriendly}`);
    console.log();
    console.log('State:');
    console.log('  chainId            =', state.chainId);
    console.log('  currentOriginNonce =', state.currentOriginNonce.toString());
    console.log('  pauseFlags         =', state.pauseFlags);
}
