import {
    type Address,
    type Cell,
    type Contract,
    type ContractProvider,
    Dictionary,
    SendMode,
    type Sender,
    beginCell,
    contractAddress,
} from '@ton/core';

export const Opcodes = {
    INIT_TRANSFER_NATIVE: 0x6e910001,
    FIN_TRANSFER: 0x6e910002,
    DEPLOY_TOKEN: 0x6e910003,
    REGISTER_JETTON: 0x6e910004,
    LOG_METADATA: 0x6e910005,
    SET_PAUSE: 0x6e910010,
    SET_ADMIN: 0x6e910012,
    ACCEPT_ADMIN: 0x6e910013,
    UPGRADE_CODE: 0x6e910014,
    SET_JETTON_CODE: 0x6e910015,
    INIT_TRANSFER_JETTON_FWD: 0x6e910020,
    TRANSFER_NOTIFICATION: 0x7362d09c,
    TEP74_TRANSFER: 0xf8a7ea5,
    PROVIDE_WALLET_ADDRESS: 0x2c76b973, // TEP-89 (sent to master)
    TAKE_WALLET_ADDRESS: 0xd1735400, // TEP-89 (reply from master)
    BURN_COMPLETE: 0x6e910030,
    BURN_ABORTED: 0x6e910031,
    TEP74_BURN: 0x595f07bc,
    BURN_NOTIFICATION: 0x7bdd97de,
    EXCESSES: 0xd53276db,
    EVENT_INIT_TRANSFER: 0x99000001,
    EVENT_FIN_TRANSFER: 0x99000002,
    EVENT_DEPLOY_TOKEN: 0x99000003,
    EVENT_LOG_METADATA: 0x99000004,
    EVENT_REGISTER_JETTON: 0x99000006,
    EVENT_PAUSE_STATE: 0x99000010,
    EVENT_ADMIN: 0x99000011,
    EVENT_FIN_STUCK: 0x99000020,
    EVENT_BRIDGE_MINTED_REFUND: 0x99000022,
    EVENT_DEPLOY_TOKEN_FAILED: 0x99000023,
    EVENT_LOG_METADATA_FAILED: 0x99000024,
};

export const TON_CHAIN_ID = 12;

export const PauseFlags = {
    INIT: 1,
    FIN: 2,
    DEPLOY: 4,
    ALL: 0xff,
};

export const JettonKind = {
    BRIDGE_MINTED: 0,
    LOCKED_NATIVE: 1,
};

export type TransferMessagePayload = {
    destinationNonce: bigint;
    originChain: number;
    originNonce: bigint;
    tokenAddress: Buffer;
    amount: bigint;
    recipient: Buffer;
    feeRecipient: string | null;
    message: Buffer | null;
};

// Wire format is bit-for-bit compatible with the canonical EVM encoder at
// `evm/src/omni-bridge/contracts/OmniBridge.sol:284-303`:
//   - `fee_recipient` uses TAG-based encoding (0x00 None | 0x01 + Borsh-string Some)
//   - `message` uses PRESENCE-based encoding (nothing if empty, else Borsh-bytes
//     WITHOUT a leading tag byte). Deliberate divergence from stdlib Borsh's
//     Option<Vec<u8>> to save 1 byte per payload and match the reference contract.
export function encodeTransferMessagePayload(p: TransferMessagePayload): Buffer {
    return encodeTransferMessagePayloadWithChainId(p, TON_CHAIN_ID);
}

// Test-only: lets us forge payloads with a wrong chain_id to exercise the
// double-bind assertion in fin_transfer. Do NOT use outside tests.
export function encodeTransferMessagePayloadWithChainId(
    p: TransferMessagePayload,
    destChainId: number,
): Buffer {
    if (p.tokenAddress.length !== 32) throw new Error('tokenAddress must be 32 bytes');
    if (p.recipient.length !== 32) throw new Error('recipient must be 32 bytes');

    const chunks: Buffer[] = [];
    chunks.push(Buffer.from([0])); // PayloadType::TransferMessage
    chunks.push(u64le(p.destinationNonce));
    chunks.push(Buffer.from([p.originChain & 0xff]));
    chunks.push(u64le(p.originNonce));
    chunks.push(Buffer.from([destChainId & 0xff]));
    chunks.push(p.tokenAddress);
    chunks.push(u128le(p.amount));
    chunks.push(Buffer.from([destChainId & 0xff]));
    chunks.push(p.recipient);

    if (p.feeRecipient === null) {
        chunks.push(Buffer.from([0]));
    } else {
        chunks.push(Buffer.from([1]));
        const s = Buffer.from(p.feeRecipient, 'utf8');
        chunks.push(u32le(BigInt(s.length)));
        chunks.push(s);
    }

    if (p.message !== null) {
        chunks.push(u32le(BigInt(p.message.length)));
        chunks.push(p.message);
    }

    return Buffer.concat(chunks);
}

export type MetadataPayload = {
    nearTokenId: string;
    name: string;
    symbol: string;
    decimals: number;
};

export function encodeMetadataPayload(p: MetadataPayload): Buffer {
    const parts: Buffer[] = [];
    parts.push(Buffer.from([1])); // PayloadType::Metadata
    for (const str of [p.nearTokenId, p.name, p.symbol]) {
        const b = Buffer.from(str, 'utf8');
        parts.push(u32le(BigInt(b.length)));
        parts.push(b);
    }
    parts.push(Buffer.from([p.decimals & 0xff]));
    return Buffer.concat(parts);
}

function u32le(v: bigint): Buffer {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(Number(v & 0xffffffffn), 0);
    return b;
}
function u64le(v: bigint): Buffer {
    const b = Buffer.alloc(8);
    b.writeBigUInt64LE(v, 0);
    return b;
}
function u128le(v: bigint): Buffer {
    const b = Buffer.alloc(16);
    for (let i = 0; i < 16; i++) b[i] = Number((v >> BigInt(i * 8)) & 0xffn);
    return b;
}

// Pack arbitrary byte buffer into a single cell (max ~127 bytes = 1016 bits).
export function bytesToCell(b: Buffer): Cell {
    if (b.length > 127) throw new Error(`payload too big for single cell: ${b.length}B`);
    const builder = beginCell();
    for (const byte of b) builder.storeUint(byte, 8);
    return builder.endCell();
}

export type OmniBridgeConfig = {
    admin: Address;
    nearBridgeDerivedAddr: bigint;
    chainId: number;
    jettonMasterCode: Cell;
    jettonWalletCode: Cell;
    currentOriginNonce?: bigint;
    pauseFlags?: number;
};

export function omniBridgeConfigToCell(c: OmniBridgeConfig): Cell {
    const shelf = beginCell()
        .storeDict(Dictionary.empty()) // jettons
        .storeDict(Dictionary.empty()) // masterByLockerJw
        .storeRef(c.jettonMasterCode)
        .storeRef(c.jettonWalletCode)
        .endCell();

    return beginCell()
        .storeAddress(c.admin)
        .storeAddress(null) // pendingAdmin = addr_none$00 (2 bits)
        .storeUint(c.pauseFlags ?? 0, 8)
        .storeUint(c.nearBridgeDerivedAddr, 160)
        .storeUint(c.chainId, 8)
        .storeUint(c.currentOriginNonce ?? 0n, 64)
        .storeDict(Dictionary.empty()) // completedTransfers
        .storeRef(shelf)
        .storeDict(Dictionary.empty()) // pendingRegistration: master → PendingRegistration cell
        .storeDict(Dictionary.empty()) // pendingBurns: burn queryId → PendingBurn cell
        .storeUint(0n, 64) // burnQueryCounter
        .endCell();
}

export class OmniBridge implements Contract {
    constructor(
        readonly address: Address,
        readonly init?: { code: Cell; data: Cell },
    ) {}

    static createFromConfig(config: OmniBridgeConfig, code: Cell, workchain = 0) {
        const data = omniBridgeConfigToCell(config);
        const init = { code, data };
        return new OmniBridge(contractAddress(workchain, init), init);
    }

    static createFromAddress(a: Address) {
        return new OmniBridge(a);
    }

    async sendDeploy(provider: ContractProvider, via: Sender, value: bigint) {
        await provider.internal(via, {
            value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body: beginCell().endCell(),
        });
    }

    async sendFinTransfer(
        provider: ContractProvider,
        via: Sender,
        opts: {
            value: bigint;
            queryId?: bigint;
            sigR: bigint;
            sigS: bigint;
            sigV: number;
            payload: Buffer;
        },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.FIN_TRANSFER, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeUint(opts.sigR, 256)
            .storeUint(opts.sigS, 256)
            .storeUint(opts.sigV, 8)
            .storeRef(bytesToCell(opts.payload))
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendInitTransferNative(
        provider: ContractProvider,
        via: Sender,
        opts: {
            value: bigint;
            queryId?: bigint;
            amount: bigint;
            fee: bigint;
            nativeFee: bigint;
            recipient: Buffer;
            message?: Buffer;
        },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.INIT_TRANSFER_NATIVE, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeUint(opts.amount, 128)
            .storeUint(opts.fee, 128)
            .storeUint(opts.nativeFee, 128)
            .storeRef(bytesToCell(opts.recipient))
            .storeRef(bytesToCell(opts.message ?? Buffer.alloc(0)))
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendDeployToken(
        provider: ContractProvider,
        via: Sender,
        opts: {
            value: bigint;
            queryId?: bigint;
            sigR: bigint;
            sigS: bigint;
            sigV: number;
            metadataPayload: Buffer;
            contentRef?: Cell;
        },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.DEPLOY_TOKEN, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeUint(opts.sigR, 256)
            .storeUint(opts.sigS, 256)
            .storeUint(opts.sigV, 8)
            .storeRef(bytesToCell(opts.metadataPayload))
            .storeRef(opts.contentRef ?? beginCell().endCell())
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendRegisterJetton(
        provider: ContractProvider,
        via: Sender,
        opts: {
            value: bigint;
            queryId?: bigint;
            kind: number;
            master: Address;
            lockerJw: Address;
            decimals: number;
        },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.REGISTER_JETTON, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeUint(opts.kind, 8)
            .storeAddress(opts.master)
            .storeAddress(opts.lockerJw)
            .storeUint(opts.decimals, 8)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendLogMetadata(
        provider: ContractProvider,
        via: Sender,
        opts: { value: bigint; queryId?: bigint; master: Address },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.LOG_METADATA, 32)
            .storeUint(opts.queryId ?? 0n, 64)
            .storeAddress(opts.master)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendSetPause(
        provider: ContractProvider,
        via: Sender,
        opts: { value: bigint; flags: number },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.SET_PAUSE, 32)
            .storeUint(0, 64)
            .storeUint(opts.flags, 8)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendSetAdmin(
        provider: ContractProvider,
        via: Sender,
        opts: { value: bigint; newAdmin: Address },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.SET_ADMIN, 32)
            .storeUint(0, 64)
            .storeAddress(opts.newAdmin)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendAcceptAdmin(provider: ContractProvider, via: Sender, opts: { value: bigint }) {
        const body = beginCell().storeUint(Opcodes.ACCEPT_ADMIN, 32).storeUint(0, 64).endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendUpgradeCode(
        provider: ContractProvider,
        via: Sender,
        opts: { value: bigint; newCode: Cell },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.UPGRADE_CODE, 32)
            .storeUint(0, 64)
            .storeRef(opts.newCode)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async sendSetJettonCode(
        provider: ContractProvider,
        via: Sender,
        opts: { value: bigint; newJettonMasterCode: Cell; newJettonWalletCode: Cell },
    ) {
        const body = beginCell()
            .storeUint(Opcodes.SET_JETTON_CODE, 32)
            .storeUint(0, 64)
            .storeRef(opts.newJettonMasterCode)
            .storeRef(opts.newJettonWalletCode)
            .endCell();
        await provider.internal(via, {
            value: opts.value,
            sendMode: SendMode.PAY_GAS_SEPARATELY,
            body,
        });
    }

    async getState(provider: ContractProvider): Promise<{
        nearBridgeDerivedAddr: bigint;
        chainId: number;
        currentOriginNonce: bigint;
        pauseFlags: number;
    }> {
        const r = await provider.get('getState', []);
        return {
            nearBridgeDerivedAddr: r.stack.readBigNumber(),
            chainId: r.stack.readNumber(),
            currentOriginNonce: r.stack.readBigNumber(),
            pauseFlags: r.stack.readNumber(),
        };
    }

    async getAdmin(
        provider: ContractProvider,
    ): Promise<{ admin: Address; pendingAdmin: Address | null }> {
        const r = await provider.get('getAdmin', []);
        const admin = r.stack.readAddress();
        const pending = r.stack.readAddressOpt();
        return { admin, pendingAdmin: pending };
    }

    async getIsTransferFinalised(provider: ContractProvider, nonce: bigint): Promise<boolean> {
        const r = await provider.get('isTransferFinalised', [{ type: 'int', value: nonce }]);
        return r.stack.readBoolean();
    }

    async getJetton(
        provider: ContractProvider,
        master: Address,
    ): Promise<{
        found: boolean;
        kind: number;
        lockerJw: Address;
        decimals: number;
    }> {
        const r = await provider.get('getJetton', [
            { type: 'slice', cell: beginCell().storeAddress(master).endCell() },
        ]);
        return {
            found: r.stack.readBoolean(),
            kind: r.stack.readNumber(),
            lockerJw: r.stack.readAddress(),
            decimals: r.stack.readNumber(),
        };
    }

    async getPendingRegistration(
        provider: ContractProvider,
        master: Address,
    ): Promise<{
        found: boolean;
        caller: Address;
    }> {
        const r = await provider.get('getPendingRegistration', [
            { type: 'slice', cell: beginCell().storeAddress(master).endCell() },
        ]);
        return {
            found: r.stack.readBoolean(),
            caller: r.stack.readAddress(),
        };
    }
}
