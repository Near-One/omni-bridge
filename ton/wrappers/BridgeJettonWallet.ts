import {
    type Address,
    type Cell,
    type Contract,
    type ContractProvider,
    beginCell,
    contractAddress,
} from '@ton/core';

export type BridgeJettonWalletConfig = {
    balance: bigint;
    ownerAddr: Address;
    masterAddr: Address;
    walletCode: Cell;
};

export function bridgeJettonWalletConfigToCell(c: BridgeJettonWalletConfig): Cell {
    return beginCell()
        .storeCoins(c.balance)
        .storeAddress(c.ownerAddr)
        .storeAddress(c.masterAddr)
        .storeRef(c.walletCode)
        .endCell();
}

export function bridgeJettonWalletAddress(
    walletCode: Cell,
    ownerAddr: Address,
    masterAddr: Address,
): Address {
    const data = bridgeJettonWalletConfigToCell({
        balance: 0n,
        ownerAddr,
        masterAddr,
        walletCode,
    });
    return contractAddress(0, { code: walletCode, data });
}

export class BridgeJettonWallet implements Contract {
    constructor(
        readonly address: Address,
        readonly init?: { code: Cell; data: Cell },
    ) {}

    static createFromAddress(a: Address) {
        return new BridgeJettonWallet(a);
    }

    async getWalletData(provider: ContractProvider): Promise<{
        balance: bigint;
        owner: Address;
        master: Address;
        walletCode: Cell;
    }> {
        const r = await provider.get('getWalletData', []);
        return {
            balance: r.stack.readBigNumber(),
            owner: r.stack.readAddress(),
            master: r.stack.readAddress(),
            walletCode: r.stack.readCell(),
        };
    }
}
