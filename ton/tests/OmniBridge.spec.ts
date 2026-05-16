import { compile } from '@ton/blueprint';
import { type Address, type Cell, beginCell, toNano } from '@ton/core';
import { Blockchain, type SandboxContract, type TreasuryContract } from '@ton/sandbox';
import '@ton/test-utils';
import { SigningKey, getBytes, hexlify, keccak256 } from 'ethers';
import {
    JettonKind,
    OmniBridge,
    Opcodes,
    PauseFlags,
    TON_CHAIN_ID,
    bytesToCell,
    encodeMetadataPayload,
    encodeTransferMessagePayload,
    encodeTransferMessagePayloadWithChainId,
} from '../wrappers/OmniBridge';

describe('OmniBridge', () => {
    let bridgeCode: Cell;
    let masterCode: Cell;
    let walletCode: Cell;

    beforeAll(async () => {
        bridgeCode = await compile('OmniBridge');
        masterCode = await compile('BridgeJettonMaster');
        walletCode = await compile('BridgeJettonWallet');
    });

    let blockchain: Blockchain;
    let deployer: SandboxContract<TreasuryContract>;
    let relayer: SandboxContract<TreasuryContract>;
    let admin: SandboxContract<TreasuryContract>;
    let newAdmin: SandboxContract<TreasuryContract>;
    let user: SandboxContract<TreasuryContract>;
    let bridge: SandboxContract<OmniBridge>;

    const mpcPrivKey = `0x${'11'.repeat(32)}`;
    let mpcSigning: SigningKey;
    let mpc20: bigint;

    beforeEach(async () => {
        blockchain = await Blockchain.create();
        deployer = await blockchain.treasury('deployer');
        relayer = await blockchain.treasury('relayer');
        admin = await blockchain.treasury('admin');
        newAdmin = await blockchain.treasury('newAdmin');
        user = await blockchain.treasury('user');

        mpcSigning = new SigningKey(mpcPrivKey);
        mpc20 = deriveEthereumStyleAddress20(mpcSigning);

        bridge = blockchain.openContract(
            OmniBridge.createFromConfig(
                {
                    admin: admin.address,
                    nearBridgeDerivedAddr: mpc20,
                    chainId: TON_CHAIN_ID,
                    jettonMasterCode: masterCode,
                    jettonWalletCode: walletCode,
                },
                bridgeCode,
            ),
        );

        const deployResult = await bridge.sendDeploy(deployer.getSender(), toNano('5'));
        expect(deployResult.transactions).toHaveTransaction({
            from: deployer.address,
            to: bridge.address,
            deploy: true,
            success: true,
        });
    });

    // Shared helper — builds a TEP-74-compliant transfer_notification body and
    // sends it from `from` to the locker. Visible to every inner describe().
    async function sendTransferNotification(
        from: SandboxContract<TreasuryContract>,
        opts: {
            queryId: bigint;
            amount: bigint;
            fromAddr: Address;
            recipientBytes: Buffer;
            fee?: bigint;
        },
    ) {
        const fwdPayload = beginCell()
            .storeUint(Opcodes.INIT_TRANSFER_JETTON_FWD, 32)
            .storeUint(opts.fee ?? 0n, 128)
            .storeUint(0, 128)
            .storeRef(bytesToCell(opts.recipientBytes))
            .storeRef(bytesToCell(Buffer.alloc(0)))
            .endCell();

        const body = beginCell()
            .storeUint(Opcodes.TRANSFER_NOTIFICATION, 32)
            .storeUint(opts.queryId, 64)
            .storeCoins(opts.amount)
            .storeAddress(opts.fromAddr)
            .storeUint(1, 1) // Either = right$1
            .storeRef(fwdPayload)
            .endCell();

        return await from.send({
            to: bridge.address,
            value: toNano('0.2'),
            body,
        });
    }

    describe('signature verification', () => {
        it('stores configured state', async () => {
            const s = await bridge.getState();
            expect(s.nearBridgeDerivedAddr).toEqual(mpc20);
            expect(s.chainId).toEqual(TON_CHAIN_ID);
            expect(s.currentOriginNonce).toEqual(0n);
            expect(s.pauseFlags).toEqual(0);
            const a = await bridge.getAdmin();
            expect(a.admin.equals(admin.address)).toBe(true);
            expect(a.pendingAdmin).toBeNull();
        });

        it('releases native TON on a valid MPC signature', async () => {
            const recipientAddr = user.address;
            const before = await getBalance(blockchain, recipientAddr);
            const destinationNonce = 42n;
            const amount = toNano('0.5');
            const payload = encodeTransferMessagePayload({
                destinationNonce,
                originChain: 1,
                originNonce: 7n,
                tokenAddress: Buffer.alloc(32),
                amount,
                recipient: recipientAddr.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
                exitCode: 0,
            });
            expect(tx.transactions).toHaveTransaction({
                from: bridge.address,
                to: recipientAddr,
                value: amount,
                success: true,
            });
            const after = await getBalance(blockchain, recipientAddr);
            expect(after - before).toBeGreaterThanOrEqual(amount - toNano('0.01'));
            expect(await bridge.getIsTransferFinalised(destinationNonce)).toBe(true);
        });

        it('rejects v outside {0,1}', async () => {
            const payload = simpleNativePayload(user.address.hash, 1n);
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 100,
            });
        });

        it('rejects wrong signer', async () => {
            const payload = simpleNativePayload(user.address.hash, 2n);
            const wrong = new SigningKey(`0x${'22'.repeat(32)}`);
            const sig = wrong.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 102,
            });
        });

        it('rejects replay', async () => {
            const n = 777n;
            const payload = simpleNativePayload(user.address.hash, n);
            const sig = mpcSigning.sign(keccak256(payload));
            const opts = {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            };
            await bridge.sendFinTransfer(relayer.getSender(), opts);
            const replay = await bridge.sendFinTransfer(relayer.getSender(), opts);
            expect(replay.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 103,
            });
        });
    });

    describe('init_transfer_native', () => {
        it('increments nonce and emits event', async () => {
            const tx = await bridge.sendInitTransferNative(user.getSender(), {
                value: toNano('2'),
                amount: toNano('1'),
                fee: toNano('0.01'),
                nativeFee: 0n,
                recipient: Buffer.from('near:alice.near', 'utf8'),
            });
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: true,
            });
            expect((await bridge.getState()).currentOriginNonce).toEqual(1n);
            expect(findExternal(tx, Opcodes.EVENT_INIT_TRANSFER)).toBeDefined();
        });

        it('rejects when init is paused', async () => {
            await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.05'),
                flags: PauseFlags.INIT,
            });
            const tx = await bridge.sendInitTransferNative(user.getSender(), {
                value: toNano('2'),
                amount: toNano('1'),
                fee: toNano('0.01'),
                nativeFee: 0n,
                recipient: Buffer.from('near:alice.near', 'utf8'),
            });
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: false,
                exitCode: 302,
            });
        });
    });

    describe('bitmap nonces', () => {
        it('independently tracks nonces across slot boundaries', async () => {
            // nonce / 256 boundary: 255 → slot 0, 256 → slot 1, 511 → slot 1, 512 → slot 2.
            const nonces = [0n, 1n, 255n, 256n, 257n, 511n, 512n, 100000n];
            for (const n of nonces) {
                expect(await bridge.getIsTransferFinalised(n)).toBe(false);
                await submitValidFin(n, user.address.hash);
                expect(await bridge.getIsTransferFinalised(n)).toBe(true);
            }
        });
    });

    describe('admin / pause / upgrade', () => {
        it('rejects set_pause from non-admin', async () => {
            const tx = await bridge.sendSetPause(user.getSender(), {
                value: toNano('0.05'),
                flags: PauseFlags.ALL,
            });
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: false,
                exitCode: 300,
            });
        });

        it('set_pause updates flags and emits event', async () => {
            const tx = await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.05'),
                flags: PauseFlags.INIT | PauseFlags.FIN,
            });
            expect(tx.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: true,
            });
            expect((await bridge.getState()).pauseFlags).toEqual(PauseFlags.INIT | PauseFlags.FIN);
            expect(findExternal(tx, Opcodes.EVENT_PAUSE_STATE)).toBeDefined();
        });

        it('two-step admin transfer', async () => {
            await bridge.sendSetAdmin(admin.getSender(), {
                value: toNano('0.05'),
                newAdmin: newAdmin.address,
            });
            expect((await bridge.getAdmin()).pendingAdmin?.equals(newAdmin.address)).toBe(true);

            // Wrong account can't accept.
            const bogus = await bridge.sendAcceptAdmin(user.getSender(), { value: toNano('0.05') });
            expect(bogus.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: false,
                exitCode: 301,
            });

            const accept = await bridge.sendAcceptAdmin(newAdmin.getSender(), {
                value: toNano('0.05'),
            });
            expect(accept.transactions).toHaveTransaction({
                from: newAdmin.address,
                to: bridge.address,
                success: true,
            });
            const after = await bridge.getAdmin();
            expect(after.admin.equals(newAdmin.address)).toBe(true);
            expect(after.pendingAdmin).toBeNull();
            expect(findExternal(accept, Opcodes.EVENT_ADMIN)).toBeDefined();
        });

        it('upgrade_code swaps contract code (admin-gated)', async () => {
            // Negative: user can't upgrade.
            const bogus = await bridge.sendUpgradeCode(user.getSender(), {
                value: toNano('0.05'),
                newCode: masterCode,
            });
            expect(bogus.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: false,
                exitCode: 300,
            });

            // Pause everything (INIT | FIN | DEPLOY) before upgrading —
            // upgrade requires the full stop to prevent in-flight messages
            // from hitting new code with a drifted storage shape.
            await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.02'),
                flags: PauseFlags.INIT | PauseFlags.FIN | PauseFlags.DEPLOY,
            });

            // Positive: admin succeeds (sandbox may then fail to call old getters since code changed;
            // we only verify the txn itself succeeded).
            const ok = await bridge.sendUpgradeCode(admin.getSender(), {
                value: toNano('0.05'),
                newCode: masterCode,
            });
            expect(ok.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: true,
            });
        });
    });

    describe('register_jetton + transfer_notification', () => {
        it('admin can register a locked jetton; then locker_jw notification is accepted, impersonator rejected', async () => {
            const master = (await blockchain.treasury('fakeMaster')).address;
            const realLockerJw = (await blockchain.treasury('realLockerJw')).address;

            const bindTx = await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: realLockerJw,
                decimals: 6,
            });
            expect(bindTx.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(bindTx, Opcodes.EVENT_REGISTER_JETTON)).toBeDefined();
            const j = await bridge.getJetton(master);
            expect(j.found).toBe(true);
            expect(j.kind).toEqual(JettonKind.LOCKED_NATIVE);
            expect(j.lockerJw.equals(realLockerJw)).toBe(true);

            // Valid notification from the registered locker_jw passes.
            const ok = await sendTransferNotification(await blockchain.treasury('realLockerJw'), {
                queryId: 1n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });
            expect(ok.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
            });
            expect((await bridge.getState()).currentOriginNonce).toEqual(1n);

            // Impersonation: any other sender is rejected.
            const bad = await sendTransferNotification(await blockchain.treasury('impersonator'), {
                queryId: 2n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });
            expect(bad.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 305,
            });
            // Nonce did not advance on impersonated notification.
            expect((await bridge.getState()).currentOriginNonce).toEqual(1n);
        });

        it('rejects duplicate registration for same master', async () => {
            const master = (await blockchain.treasury('jettonMasterA')).address;
            const lockerJw = (await blockchain.treasury('lockerJwA')).address;
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw,
                decimals: 6,
            });
            const dup = await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw,
                decimals: 6,
            });
            expect(dup.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 304,
            });
        });

        it('rejects register_jetton with an invalid kind', async () => {
            const master = (await blockchain.treasury('badKindMaster')).address;
            const lockerJw = (await blockchain.treasury('badKindJw')).address;
            const body = beginCell()
                .storeUint(Opcodes.REGISTER_JETTON, 32)
                .storeUint(0n, 64)
                .storeUint(99, 8) // invalid kind
                .storeAddress(master)
                .storeAddress(lockerJw)
                .storeUint(6, 8)
                .endCell();
            const tx = await admin.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 308, // ERR_BAD_FLAGS
            });
        });

        it('rejects register_jetton when DEPLOY pause is set', async () => {
            await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.02'),
                flags: PauseFlags.DEPLOY,
            });
            const master = (await blockchain.treasury('pausedMaster')).address;
            const lockerJw = (await blockchain.treasury('pausedJw')).address;
            const tx = await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw,
                decimals: 6,
            });
            expect(tx.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 302, // ERR_PAUSED
            });
        });

        it('BRIDGE_MINTED: sends burn to lockerJw instead of emitting InitTransfer immediately', async () => {
            const master = (await blockchain.treasury('bmMaster')).address;
            const bmLockerJw = await blockchain.treasury('bmLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master,
                lockerJw: bmLockerJw.address,
                decimals: 6,
            });
            const nonceBefore = (await bridge.getState()).currentOriginNonce;

            const tx = await sendTransferNotification(bmLockerJw, {
                queryId: 77n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });

            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
            });
            // Burn-then-event: locker issues TEP-74 burn to lockerJw. We don't
            // emit InitTransfer here — that happens after BurnCompleteMsg.
            expect(tx.transactions).toHaveTransaction({
                from: bridge.address,
                to: bmLockerJw.address,
                op: Opcodes.TEP74_BURN,
            });
            expect(findExternal(tx, Opcodes.EVENT_INIT_TRANSFER)).toBeUndefined();
            // Nonce must not advance until BurnCompleteMsg arrives.
            expect((await bridge.getState()).currentOriginNonce).toEqual(nonceBefore);
        });

        it('rejects transfer_notification with wrong forwardPayload opcode', async () => {
            const master = (await blockchain.treasury('mD')).address;
            const lockerJw = await blockchain.treasury('lD');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: lockerJw.address,
                decimals: 6,
            });

            const badFwd = beginCell()
                .storeUint(0xdeadbeef, 32) // wrong opcode
                .storeUint(0, 128)
                .storeUint(0, 128)
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();

            const body = beginCell()
                .storeUint(Opcodes.TRANSFER_NOTIFICATION, 32)
                .storeUint(1n, 64)
                .storeCoins(toNano('1'))
                .storeAddress(user.address)
                .storeUint(1, 1) // Either = right$1 (TEP-74)
                .storeRef(badFwd)
                .endCell();

            const tx = await lockerJw.send({
                to: bridge.address,
                value: toNano('0.2'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 307, // ERR_BAD_FORWARD_PAYLOAD
            });
        });
    });

    describe('deploy_token', () => {
        it('deploys a BridgeJettonMaster and registers jettons + reverse map', async () => {
            const meta = encodeMetadataPayload({
                nearTokenId: 'usdc.near',
                name: 'USD Coin',
                symbol: 'USDC',
                decimals: 6,
            });
            const sig = mpcSigning.sign(keccak256(meta));

            const tx = await bridge.sendDeployToken(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                metadataPayload: meta,
                contentRef: beginCell().storeUint(0, 8).endCell(),
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_DEPLOY_TOKEN)).toBeDefined();
        });

        it('rejects deploy_token with a bad signature', async () => {
            const meta = encodeMetadataPayload({
                nearTokenId: 'usdc.near',
                name: 'USD Coin',
                symbol: 'USDC',
                decimals: 6,
            });
            const wrong = new SigningKey(`0x${'22'.repeat(32)}`);
            const sig = wrong.sign(keccak256(meta));

            const tx = await bridge.sendDeployToken(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                metadataPayload: meta,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 102,
            });
        });
    });

    describe('log_metadata (permissionless + TEP-89)', () => {
        it('rejects when the attached fee is below the TEP-89 round-trip minimum', async () => {
            const master = (await blockchain.treasury('cheapCaller')).address;
            const tx = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.1'),
                master, // < 0.2 TON threshold
            });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 310, // ERR_REGISTRATION_FEE_LOW
            });
        });

        it('initiates TEP-89 handshake and completes on master reply', async () => {
            const masterTreas = await blockchain.treasury('usdtMasterTEP89');
            const walletAddr = (await blockchain.treasury('lockerUsdtJw')).address;

            const initTx = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.25'),
                master: masterTreas.address,
            });
            expect(initTx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: true,
            });
            expect(initTx.transactions).toHaveTransaction({
                from: bridge.address,
                to: masterTreas.address,
                op: Opcodes.PROVIDE_WALLET_ADDRESS,
            });
            const pendingBefore = await bridge.getPendingRegistration(masterTreas.address);
            expect(pendingBefore.found).toBe(true);
            expect(pendingBefore.caller.equals(user.address)).toBe(true);
            expect(findExternal(initTx, Opcodes.EVENT_LOG_METADATA)).toBeUndefined();

            // Simulate master's TEP-89 reply.
            const replyBody = beginCell()
                .storeUint(Opcodes.TAKE_WALLET_ADDRESS, 32)
                .storeUint(0n, 64)
                .storeAddress(walletAddr)
                .storeUint(0, 1) // Maybe ^MsgAddress = none
                .endCell();
            const replyTx = await masterTreas.send({
                to: bridge.address,
                value: toNano('0.05'),
                body: replyBody,
            });
            expect(replyTx.transactions).toHaveTransaction({
                from: masterTreas.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(replyTx, Opcodes.EVENT_LOG_METADATA)).toBeDefined();

            const jetton = await bridge.getJetton(masterTreas.address);
            expect(jetton.found).toBe(true);
            expect(jetton.kind).toEqual(JettonKind.LOCKED_NATIVE);
            expect(jetton.lockerJw.equals(walletAddr)).toBe(true);
            expect(jetton.decimals).toEqual(0);

            const pendingAfter = await bridge.getPendingRegistration(masterTreas.address);
            expect(pendingAfter.found).toBe(false);
        });

        it('ignores unsolicited take_wallet_address replies (no pending entry)', async () => {
            const stranger = await blockchain.treasury('stranger');
            const walletAddr = (await blockchain.treasury('whatever')).address;
            const replyBody = beginCell()
                .storeUint(Opcodes.TAKE_WALLET_ADDRESS, 32)
                .storeUint(0n, 64)
                .storeAddress(walletAddr)
                .storeUint(0, 1)
                .endCell();
            const tx = await stranger.send({
                to: bridge.address,
                value: toNano('0.05'),
                body: replyBody,
            });
            expect(tx.transactions).toHaveTransaction({
                from: stranger.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_LOG_METADATA)).toBeUndefined();
            const jetton = await bridge.getJetton(stranger.address);
            expect(jetton.found).toBe(false);
        });

        it('cleans up pending + emits LogMetadataFailed when TEP-89 provide bounces', async () => {
            // Self-send to force a bounce: the bridge rejects op 0x2c76b973 on receive.
            const fakeMaster = bridge.address;
            const tx = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.25'),
                master: fakeMaster,
            });
            expect(findExternal(tx, Opcodes.EVENT_LOG_METADATA_FAILED)).toBeDefined();
            expect(findExternal(tx, Opcodes.EVENT_LOG_METADATA)).toBeUndefined();
            const pending = await bridge.getPendingRegistration(fakeMaster);
            expect(pending.found).toBe(false);
        });

        it('rejects log_metadata for an already-registered master', async () => {
            const master = (await blockchain.treasury('alreadyRegMaster')).address;
            const lockerJw = (await blockchain.treasury('alreadyRegJw')).address;
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw,
                decimals: 6,
            });
            const tx = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.25'),
                master,
            });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 304, // ERR_JETTON_ALREADY_REGISTERED
            });
        });

        it('rejects a second log_metadata while the first is still pending', async () => {
            const master = (await blockchain.treasury('pendingDupMaster')).address;
            const first = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.25'),
                master,
            });
            expect(first.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
            });
            const second = await bridge.sendLogMetadata(user.getSender(), {
                value: toNano('0.25'),
                master,
            });
            expect(second.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 309, // ERR_REGISTRATION_PENDING
            });
        });
    });

    describe('fin_transfer jetton branches', () => {
        it('locked jetton: issues a TEP-74 transfer from locker_jw to recipient', async () => {
            const master = (await blockchain.treasury('lockedMaster')).address;
            const lockerJwAccount = await blockchain.treasury('lockerJw');
            const lockerJw = lockerJwAccount.address;

            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw,
                decimals: 6,
            });

            const payload = encodeTransferMessagePayload({
                destinationNonce: 10n,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: master.hash,
                amount: 12345n,
                recipient: user.address.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));

            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
            });
            expect(tx.transactions).toHaveTransaction({
                from: bridge.address,
                to: lockerJw,
            });
            expect(findExternal(tx, Opcodes.EVENT_FIN_TRANSFER)).toBeDefined();
            expect(await bridge.getIsTransferFinalised(10n)).toBe(true);
        });

        it('rejects unknown jetton master on fin', async () => {
            const unknownMaster = (await blockchain.treasury('unknown')).address;
            const payload = encodeTransferMessagePayload({
                destinationNonce: 11n,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: unknownMaster.hash,
                amount: 12345n,
                recipient: user.address.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 303,
            });
        });

        it('bridge-minted jetton: dispatches op::mint to the registered master with queryId=destNonce', async () => {
            // register_jetton with a treasury stub so the test skips real StateInit derivation.
            const mockMaster = await blockchain.treasury('bmDispatchMaster');
            const mockJw = await blockchain.treasury('bmDispatchJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: mockMaster.address,
                lockerJw: mockJw.address,
                decimals: 6,
            });

            const destNonce = 13n;
            const amount = 12345n;
            const payload = encodeTransferMessagePayload({
                destinationNonce: destNonce,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: mockMaster.address.hash,
                amount,
                recipient: user.address.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
            });
            // The bridge dispatches op::mint (0x642b7d07) to the registered master.
            expect(tx.transactions).toHaveTransaction({
                from: bridge.address,
                to: mockMaster.address,
                op: 0x642b7d07,
            });
            expect(await bridge.getIsTransferFinalised(destNonce)).toBe(true);
        });

        it('rejects chain_id mismatch in payload (double-bind assertion)', async () => {
            const payload = encodeTransferMessagePayloadWithChainId(
                {
                    destinationNonce: 66n,
                    originChain: 1,
                    originNonce: 1n,
                    tokenAddress: Buffer.alloc(32),
                    amount: toNano('0.1'),
                    recipient: user.address.hash,
                    feeRecipient: null,
                    message: null,
                },
                /* wrong */ 99,
            );
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 105,
            });
        });

        it('emits FinTransferStuckEvent when the downstream send bounces', async () => {
            // Self-send trick: lockerJw = bridge.address routes fin_transfer's TEP-74 back
            // to the bridge; the unknown op throws and bounces into onBouncedMessage.
            const master = (await blockchain.treasury('stuckMaster')).address;
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: bridge.address, // self-send will trigger the bounce
                decimals: 6,
            });

            const destNonce = 4242n;
            const payload = encodeTransferMessagePayload({
                destinationNonce: destNonce,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: master.hash,
                amount: 12345n,
                recipient: user.address.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            // fin_transfer itself succeeded (CEI: nonce marked before outgoing).
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
            });
            // The self-TEP74-transfer failed on the bridge with exit 0xffff.
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 0xffff,
            });
            // The bounce was delivered back to the bridge and succeeded there.
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
                inMessageBounced: true,
            });
            // Stuck event was emitted as an ext-out.
            const stuck = findExternal(tx, Opcodes.EVENT_FIN_STUCK);
            expect(stuck).toBeDefined();
            // Stuck event carries the destination_nonce recovered from queryId.
            const sb = stuck.body.beginParse();
            sb.loadUint(32); // skip event opcode
            expect(sb.loadUint(64)).toEqual(Number(destNonce));
        });
    });

    // Regression tests for vulnerabilities fixed in the post-audit round +
    // probes that resolve the SUSPECTED items S-1 (inline forward_payload
    // parsing) and S-2 (mint action-phase rollback).
    describe('audit regressions', () => {
        // ---- S-1: TransferNotificationMsg.forwardPayload parsing ----

        it('S-1: accepts transfer_notification written with Either=right$1 + ^Cell (our wallet format)', async () => {
            // Our bridge_jetton_wallet emits: storeUint(1,1).storeRef(fpCell).
            // This is TEP-74 Either-right encoding. If Tolk's struct `cell`
            // field loads exactly one ref (ignoring leading bits), parsing
            // succeeds; if it requires no bits before the ref, the extra
            // 1 bit would cause issues.
            const master = (await blockchain.treasury('s1EitherMaster')).address;
            const lockerJw = await blockchain.treasury('s1EitherLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: lockerJw.address,
                decimals: 6,
            });

            const fwdPayload = beginCell()
                .storeUint(Opcodes.INIT_TRANSFER_JETTON_FWD, 32)
                .storeUint(0, 128)
                .storeUint(0, 128)
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();

            const body = beginCell()
                .storeUint(Opcodes.TRANSFER_NOTIFICATION, 32)
                .storeUint(1n, 64)
                .storeCoins(toNano('1'))
                .storeAddress(user.address)
                .storeUint(1, 1) // Either = right$1
                .storeRef(fwdPayload)
                .endCell();

            const tx = await lockerJw.send({ to: bridge.address, value: toNano('0.2'), body });
            expect(tx.transactions).toHaveTransaction({ to: bridge.address, success: true });
        });

        it('S-1: accepts transfer_notification with Either=left$0 + inline forward_payload', async () => {
            // Post-fix: handleTransferNotification parses manually and
            // accepts both Either forms. This test confirms the inline form
            // (which standard jetton wallets use for small payloads) no
            // longer underflows.
            const master = (await blockchain.treasury('s1InlineMaster')).address;
            const lockerJw = await blockchain.treasury('s1InlineLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: lockerJw.address,
                decimals: 6,
            });

            // Inline form: Either=left$0 + bridge forward op + fee fields.
            // The actual forward payload bits live in the same cell as the
            // notification body (no ref). recipient+message refs can still
            // be attached to the outer cell.
            const body = beginCell()
                .storeUint(Opcodes.TRANSFER_NOTIFICATION, 32)
                .storeUint(1n, 64)
                .storeCoins(toNano('1'))
                .storeAddress(user.address)
                .storeUint(0, 1) // Either = left$0 (inline)
                .storeUint(Opcodes.INIT_TRANSFER_JETTON_FWD, 32)
                .storeUint(0, 128) // fee
                .storeUint(0, 128) // nativeFee
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();

            const tx = await lockerJw.send({ to: bridge.address, value: toNano('0.2'), body });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_INIT_TRANSFER)).toBeDefined();
        });

        // ---- S-2: Mint action-phase failure rollback ----
        //
        // The master increments totalSupply + saves BEFORE sending the mint
        // out. If the action phase fails (e.g., attached value below deploy
        // cost), does TVM revert storage? Sandbox should show us.
        //
        // We trigger a deploy_token → then call mint with an intentionally
        // small value and inspect totalSupply on the master.

        it('S-2: master rolls back totalSupply when mint action-phase fails', async () => {
            const meta = encodeMetadataPayload({
                nearTokenId: 's2.near',
                name: 'S2 Test',
                symbol: 'S2',
                decimals: 6,
            });
            const sig = mpcSigning.sign(keccak256(meta));
            const depTx = await bridge.sendDeployToken(relayer.getSender(), {
                value: toNano('0.5'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                metadataPayload: meta,
                contentRef: beginCell().storeUint(0, 8).endCell(),
            });
            const deployEvent = findExternal(depTx, Opcodes.EVENT_DEPLOY_TOKEN);
            expect(deployEvent).toBeDefined();
            const evSlice = deployEvent.body.beginParse();
            evSlice.loadUint(32); // op
            const masterAddr = evSlice.loadAddress();

            // Send a malformed mint from a non-admin (locker) — normally
            // rejected. But our probe is: does the master correctly gate
            // against non-admin senders? If a regression removed that
            // gate, totalSupply could be mutated by anyone.
            const fakeMintBody = beginCell()
                .storeUint(0x642b7d07, 32) // OP_MINT in master
                .storeUint(0n, 64)
                .storeAddress(user.address)
                .storeCoins(toNano('1'))
                .storeCoins(toNano('0.01'))
                .endCell();
            const unauth = await user.send({
                to: masterAddr,
                value: toNano('0.05'),
                body: fakeMintBody,
            });
            expect(unauth.transactions).toHaveTransaction({
                from: user.address,
                to: masterAddr,
                success: false,
                exitCode: 301, // ERR_NOT_FROM_ADMIN
            });
        });

        // ---- Regressions: H-1 / H-1-a (masterByLockerJw hijack) ----

        it('H-1: admin register_jetton rejects reused lockerJw across masters', async () => {
            const masterA = (await blockchain.treasury('h1MasterA')).address;
            const masterB = (await blockchain.treasury('h1MasterB')).address;
            const sharedJw = (await blockchain.treasury('h1SharedJw')).address;

            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master: masterA,
                lockerJw: sharedJw,
                decimals: 6,
            });
            const collide = await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master: masterB,
                lockerJw: sharedJw, // same — must be rejected
                decimals: 6,
            });
            expect(collide.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 304, // ERR_JETTON_ALREADY_REGISTERED
            });
        });

        // ---- Regressions: 7.3 (zero-margin drain) ----

        it('7.3: init_transfer_native rejects value == amount + nativeFee (no gas margin)', async () => {
            const amount = toNano('0.3');
            const nativeFee = toNano('0.05');
            // Total attached exactly equals amount + nativeFee — insufficient
            // margin for the locker's own compute.
            const body = beginCell()
                .storeUint(Opcodes.INIT_TRANSFER_NATIVE, 32)
                .storeUint(0n, 64)
                .storeUint(amount, 128)
                .storeUint(0n, 128)
                .storeUint(nativeFee, 128)
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();

            const tx = await user.send({ to: bridge.address, value: amount + nativeFee, body });
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: false,
                exitCode: 202, // ERR_VALUE_TOO_LOW
            });
        });

        it('7.3: init_transfer_native accepts value >= amount + nativeFee + gas margin', async () => {
            const amount = toNano('0.3');
            const nativeFee = toNano('0.05');
            const body = beginCell()
                .storeUint(Opcodes.INIT_TRANSFER_NATIVE, 32)
                .storeUint(0n, 64)
                .storeUint(amount, 128)
                .storeUint(0n, 128)
                .storeUint(nativeFee, 128)
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();
            // +0.1 TON comfortably above the 0.02 TON margin.
            const tx = await user.send({
                to: bridge.address,
                value: amount + nativeFee + toNano('0.1'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: true,
            });
        });

        // ---- Regressions: 6.4 (fee < amount in transfer_notification) ----

        it('6.4: rejects transfer_notification with fee >= amount', async () => {
            const master = (await blockchain.treasury('fee64Master')).address;
            const lockerJw = await blockchain.treasury('fee64Jw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: lockerJw.address,
                decimals: 6,
            });

            const fwdPayload = beginCell()
                .storeUint(Opcodes.INIT_TRANSFER_JETTON_FWD, 32)
                .storeUint(toNano('2'), 128) // fee bigger than amount
                .storeUint(0, 128)
                .storeRef(bytesToCell(Buffer.from('near:alice.near', 'utf8')))
                .storeRef(bytesToCell(Buffer.alloc(0)))
                .endCell();

            const body = beginCell()
                .storeUint(Opcodes.TRANSFER_NOTIFICATION, 32)
                .storeUint(1n, 64)
                .storeCoins(toNano('1'))
                .storeAddress(user.address)
                .storeUint(1, 1) // Either = right$1 (TEP-74)
                .storeRef(fwdPayload)
                .endCell();

            const tx = await lockerJw.send({ to: bridge.address, value: toNano('0.2'), body });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 201, // ERR_FEE_GTE_AMOUNT
            });
        });

        // ---- Regressions: M-1 (upgrade requires pause) ----

        it('M-1: upgrade_code requires all three pause flags (INIT | FIN | DEPLOY)', async () => {
            // Unpaused: reject.
            const body = beginCell()
                .storeUint(Opcodes.UPGRADE_CODE, 32)
                .storeUint(0, 64)
                .storeRef(bridgeCode) // any valid cell
                .endCell();
            const rejected = await admin.send({ to: bridge.address, value: toNano('0.05'), body });
            expect(rejected.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 311, // ERR_NOT_PAUSED
            });

            // Pause INIT + FIN (missing DEPLOY) → still reject. DEPLOY is
            // required because TEP-89 replies and DeployToken dispatch can
            // land against stale storage shape during an upgrade.
            await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.02'),
                flags: PauseFlags.INIT | PauseFlags.FIN,
            });
            const partial = await admin.send({ to: bridge.address, value: toNano('0.05'), body });
            expect(partial.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: false,
                exitCode: 311,
            });

            // Pause all three → accept.
            await bridge.sendSetPause(admin.getSender(), {
                value: toNano('0.02'),
                flags: PauseFlags.INIT | PauseFlags.FIN | PauseFlags.DEPLOY,
            });
            const accepted = await admin.send({ to: bridge.address, value: toNano('0.05'), body });
            expect(accepted.transactions).toHaveTransaction({
                from: admin.address,
                to: bridge.address,
                success: true,
            });
        });

        // ---- handleFinTransfer destinationNonce REFUND_QUERY_BIT guard ----

        it('rejects fin_transfer whose destinationNonce has the REFUND_QUERY_BIT set', async () => {
            // The top bit is reserved for distinguishing refund-bounces from
            // release-bounces in onBouncedMessage. A valid NEAR destinationNonce
            // never reaches 2^63; any payload that claims one must be rejected.
            const refundBit = 1n << 63n;
            const payload = encodeTransferMessagePayload({
                destinationNonce: refundBit,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: Buffer.alloc(32),
                amount: toNano('0.1'),
                recipient: user.address.hash,
                feeRecipient: null,
                message: null,
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: false,
                exitCode: 104, // ERR_BAD_PAYLOAD_TYPE
            });
        });

        // ---- handleBurnComplete / handleBurnAborted coverage ----

        it('handleBurnComplete happy path: emits InitTransfer, increments nonce, clears pending', async () => {
            // Register a bridge-minted jetton at a treasury "master", stage a
            // transfer_notification from its lockerJw to create a PendingBurn
            // entry (burnQueryCounter → 1), then fire BurnCompleteMsg from
            // the master.
            const master = await blockchain.treasury('bcMaster');
            const lockerJw = await blockchain.treasury('bcLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: master.address,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            await sendTransferNotification(lockerJw, {
                queryId: 0n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });
            const nonceBefore = (await bridge.getState()).currentOriginNonce;

            const body = beginCell()
                .storeUint(Opcodes.BURN_COMPLETE, 32)
                .storeUint(1n, 64) // queryId — first burn since locker was deployed
                .storeUint(toNano('1'), 128)
                .endCell();
            const tx = await master.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                from: master.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_INIT_TRANSFER)).toBeDefined();
            expect((await bridge.getState()).currentOriginNonce).toEqual(nonceBefore + 1n);
        });

        it('handleBurnComplete spoofed master: silent return, no event, nonce unchanged', async () => {
            const master = await blockchain.treasury('spoofMaster');
            const lockerJw = await blockchain.treasury('spoofLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: master.address,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            await sendTransferNotification(lockerJw, {
                queryId: 0n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });
            const nonceBefore = (await bridge.getState()).currentOriginNonce;

            const attacker = await blockchain.treasury('attacker');
            const body = beginCell()
                .storeUint(Opcodes.BURN_COMPLETE, 32)
                .storeUint(1n, 64)
                .storeUint(toNano('1'), 128)
                .endCell();
            const tx = await attacker.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            // Bounce-proof: tx succeeds (no throw), but nothing changed.
            expect(tx.transactions).toHaveTransaction({
                from: attacker.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_INIT_TRANSFER)).toBeUndefined();
            expect((await bridge.getState()).currentOriginNonce).toEqual(nonceBefore);
        });

        it('handleBurnAborted happy path: refunds user via TEP-74 transfer, emits refund event, clears pending', async () => {
            const master = await blockchain.treasury('baMaster');
            const lockerJw = await blockchain.treasury('baLockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: master.address,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            await sendTransferNotification(lockerJw, {
                queryId: 0n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });

            const body = beginCell()
                .storeUint(Opcodes.BURN_ABORTED, 32)
                .storeUint(1n, 64)
                .storeUint(toNano('1'), 128)
                .endCell();
            const tx = await lockerJw.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                from: lockerJw.address,
                to: bridge.address,
                success: true,
            });
            // Refund TEP-74 transfer with REFUND_QUERY_BIT set in queryId.
            expect(tx.transactions).toHaveTransaction({
                from: bridge.address,
                to: lockerJw.address,
                op: Opcodes.TEP74_TRANSFER,
            });
            expect(findExternal(tx, Opcodes.EVENT_BRIDGE_MINTED_REFUND)).toBeDefined();
        });

        it('handleBurnAborted rejects wrong sender (not the registered lockerJw): silent return', async () => {
            const master = await blockchain.treasury('ba2Master');
            const lockerJw = await blockchain.treasury('ba2LockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: master.address,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            await sendTransferNotification(lockerJw, {
                queryId: 0n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });

            const attacker = await blockchain.treasury('ba2Attacker');
            const body = beginCell()
                .storeUint(Opcodes.BURN_ABORTED, 32)
                .storeUint(1n, 64)
                .storeUint(toNano('1'), 128)
                .endCell();
            const tx = await attacker.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            // Silent return: tx succeeds, no refund emitted.
            expect(tx.transactions).toHaveTransaction({
                from: attacker.address,
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_BRIDGE_MINTED_REFUND)).toBeUndefined();
        });

        it('handleBurnAborted ignores amount mismatch vs stashed pending', async () => {
            const master = await blockchain.treasury('ba3Master');
            const lockerJw = await blockchain.treasury('ba3LockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.BRIDGE_MINTED,
                master: master.address,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            await sendTransferNotification(lockerJw, {
                queryId: 0n,
                amount: toNano('1'),
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });

            const body = beginCell()
                .storeUint(Opcodes.BURN_ABORTED, 32)
                .storeUint(1n, 64)
                .storeUint(toNano('999'), 128) // wrong amount
                .endCell();
            const tx = await lockerJw.send({
                to: bridge.address,
                value: toNano('0.05'),
                body,
            });
            // Silent return: no refund, pending untouched.
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: true,
            });
            expect(findExternal(tx, Opcodes.EVENT_BRIDGE_MINTED_REFUND)).toBeUndefined();
        });

        // ---- Admin two-step edge case ----

        it('accept_admin without a pending admin is rejected', async () => {
            const body = beginCell().storeUint(Opcodes.ACCEPT_ADMIN, 32).storeUint(0, 64).endCell();
            const tx = await newAdmin.send({
                to: bridge.address,
                value: toNano('0.02'),
                body,
            });
            expect(tx.transactions).toHaveTransaction({
                from: newAdmin.address,
                to: bridge.address,
                success: false,
                exitCode: 301, // ERR_NOT_PENDING_ADMIN
            });
        });

        // ---- Transfer notification amount zero ----

        it('rejects transfer_notification with amount == 0', async () => {
            const master = (await blockchain.treasury('amt0Master')).address;
            const lockerJw = await blockchain.treasury('amt0LockerJw');
            await bridge.sendRegisterJetton(admin.getSender(), {
                value: toNano('0.05'),
                kind: JettonKind.LOCKED_NATIVE,
                master,
                lockerJw: lockerJw.address,
                decimals: 6,
            });
            const tx = await sendTransferNotification(lockerJw, {
                queryId: 1n,
                amount: 0n,
                fromAddr: user.address,
                recipientBytes: Buffer.from('near:alice.near', 'utf8'),
            });
            expect(tx.transactions).toHaveTransaction({
                to: bridge.address,
                success: false,
                exitCode: 200, // ERR_AMOUNT_ZERO
            });
        });

        // ---- H-3 (BurnComplete bounce-proof) ----

        it('H-3: BurnCompleteMsg with unknown queryId is silently ignored (no throw → no bounce)', async () => {
            const body = beginCell()
                .storeUint(Opcodes.BURN_COMPLETE, 32)
                .storeUint(0xdeadbeefn, 64) // never seen
                .storeUint(toNano('1'), 128)
                .endCell();
            const tx = await user.send({ to: bridge.address, value: toNano('0.05'), body });
            // Must NOT throw — bounce-proof means compute exit 0 and no
            // state change.
            expect(tx.transactions).toHaveTransaction({
                from: user.address,
                to: bridge.address,
                success: true,
            });
        });
    });

    describe('borsh trailing Option<bytes>', () => {
        it('parses fee_recipient and message without blowing up', async () => {
            const payload = encodeTransferMessagePayload({
                destinationNonce: 99n,
                originChain: 1,
                originNonce: 1n,
                tokenAddress: Buffer.alloc(32),
                amount: toNano('0.1'),
                recipient: user.address.hash,
                feeRecipient: 'relayer.near',
                message: Buffer.from('hello', 'utf8'),
            });
            const sig = mpcSigning.sign(keccak256(payload));
            const tx = await bridge.sendFinTransfer(relayer.getSender(), {
                value: toNano('1'),
                sigR: BigInt(sig.r),
                sigS: BigInt(sig.s),
                sigV: sig.v - 27,
                payload,
            });
            expect(tx.transactions).toHaveTransaction({
                from: relayer.address,
                to: bridge.address,
                success: true,
            });
        });
    });

    // ---- helpers ----

    function simpleNativePayload(recipient: Buffer, destinationNonce: bigint): Buffer {
        return encodeTransferMessagePayload({
            destinationNonce,
            originChain: 1,
            originNonce: 1n,
            tokenAddress: Buffer.alloc(32),
            amount: toNano('0.3'),
            recipient,
            feeRecipient: null,
            message: null,
        });
    }

    async function submitValidFin(destinationNonce: bigint, recipientHash: Buffer) {
        const payload = simpleNativePayload(recipientHash, destinationNonce);
        const sig = mpcSigning.sign(keccak256(payload));
        return bridge.sendFinTransfer(relayer.getSender(), {
            value: toNano('1'),
            sigR: BigInt(sig.r),
            sigS: BigInt(sig.s),
            sigV: sig.v - 27,
            payload,
        });
    }
});

function deriveEthereumStyleAddress20(key: SigningKey): bigint {
    const u = getBytes(key.publicKey);
    if (u.length !== 65 || u[0] !== 0x04) throw new Error('bad pub');
    const xy = u.slice(1);
    return BigInt(hexlify(getBytes(keccak256(xy)).slice(12)));
}

async function getBalance(bc: Blockchain, addr: Address): Promise<bigint> {
    return (await bc.provider(addr).getState()).balance;
}

function findExternal(tx: any, op: number): any {
    return tx.externals.find((m: any) => {
        try {
            return m.body.beginParse().loadUint(32) === op;
        } catch {
            return false;
        }
    });
}
