import {
    type Address,
    type Cell,
    type Contract,
    type ContractProvider,
    SendMode,
    type Sender,
    beginCell,
    contractAddress,
} from '@ton/core';

export const JettonMasterOps = {
    MINT: 0x642b7d07,
    INTERNAL_TRANSFER: 0x178d4519,
};

export type BridgeJettonMasterConfig = {
    totalSupply: bigint;
    adminAddr: Address;
    walletCode: Cell;
    contentRef?: Cell;
};

export function bridgeJettonMasterConfigToCell(c: BridgeJettonMasterConfig): Cell {
    return beginCell()
        .storeCoins(c.totalSupply)
        .storeAddress(c.adminAddr)
        .storeRef(c.walletCode)
        .storeRef(c.contentRef ?? beginCell().endCell())
        .endCell();
}

export class BridgeJettonMaster implements Contract {
    constructor(
        readonly address: Address,
        readonly init?: { code: Cell; data: Cell },
    ) {}

    static createFromConfig(config: BridgeJettonMasterConfig, code: Cell, workchain = 0) {
        const data = bridgeJettonMasterConfigToCell(config);
        const init = { code, data };
        return new BridgeJettonMaster(contractAddress(workchain, init), init);
    }

    static createFromAddress(a: Address) {
        return new BridgeJettonMaster(a);
    }

    async sendDeploy(provider: ContractProvider, via: Sender, value: bigint) {
        await provider.internal(via, {
            value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body: beginCell().endCell(),
        });
    }

    async sendMint(
        provider: ContractProvider,
        via: Sender,
        opts: {
            value: bigint;
            queryId?: bigint;
            toAddr: Address;
            jettonAmount: bigint;
            forwardTonAmount?: bigint;
        },
    ) {
        const body = beginCell()
            .storeUint(JettonMasterOps.MINT, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeAddress(opts.toAddr)
            .storeCoins(opts.jettonAmount)
            .storeCoins(opts.forwardTonAmount ?? 0n)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async getJettonData(provider: ContractProvider): Promise<{
        totalSupply: bigint;
        admin: Address;
        content: Cell;
        walletCode: Cell;
    }> {
        const r = await provider.get('getJettonData', []);
        return {
            totalSupply: r.stack.readBigNumber(),
            admin: r.stack.readAddress(),
            content: r.stack.readCell(),
            walletCode: r.stack.readCell(),
        };
    }

    async getWalletAddress(provider: ContractProvider, owner: Address): Promise<Address> {
        const r = await provider.get('getWalletAddress', [
            { type: 'slice', cell: beginCell().storeAddress(owner).endCell() },
        ]);
        return r.stack.readAddress();
    }
}
