tolk 1.3

@pure
fun tvmEcrecover(hash: uint256, v: uint8, r: uint256, s: uint256): (int, int, int, int)
    asm "ECRECOVER"

@pure
fun keccak256Slice(s: slice): uint256
    asm "1 PUSHINT HASHEXT_KECCAK256"

const ERR_WRONG_OP = 0xFFFF
const ERR_BAD_V = 100
const ERR_ECDSA_FAIL = 101
const ERR_BAD_SIGNER = 102
const ERR_NONCE_USED = 103
const ERR_BAD_PAYLOAD_TYPE = 104
const ERR_BAD_DESTINATION_CHAIN = 105
const ERR_AMOUNT_ZERO = 200
const ERR_FEE_GTE_AMOUNT = 201
const ERR_VALUE_TOO_LOW = 202
const ERR_NOT_ADMIN = 300
const ERR_NOT_PENDING_ADMIN = 301
const ERR_PAUSED = 302
const ERR_UNKNOWN_JETTON = 303
const ERR_JETTON_ALREADY_REGISTERED = 304
const ERR_IMPERSONATED_JETTON = 305
const ERR_BAD_FORWARD_PAYLOAD = 307
const ERR_BAD_FLAGS = 308
const ERR_REGISTRATION_PENDING = 309
const ERR_REGISTRATION_FEE_LOW = 310
const ERR_NOT_PAUSED = 311

const PAYLOAD_TYPE_TRANSFER_MESSAGE = 0
const PAYLOAD_TYPE_METADATA = 1

const PAUSE_INIT = 1
const PAUSE_FIN = 2
const PAUSE_DEPLOY = 4
const VALID_PAUSE_FLAGS = PAUSE_INIT | PAUSE_FIN | PAUSE_DEPLOY

const JETTON_BRIDGE_MINTED = 0
const JETTON_LOCKED_NATIVE = 1

const OP_TEP74_TRANSFER = 0xf8a7ea5
const OP_TEP74_BURN = 0x595f07bc

const OP_PROVIDE_WALLET_ADDRESS = 0x2c76b973

const MIN_LOG_METADATA_FEE = 200000000 // 0.2 TON in nanotons

// Margin above `amount + nativeFee` that `handleInitTransferNative` requires,
// so the locker retains enough to cover compute + event emission + storage.
const INIT_TRANSFER_NATIVE_GAS_MARGIN = 20000000 // 0.02 TON

// Tag required in jetton-transfer forward_payload for the locker to treat the
// deposit as a bridge intent rather than a stray send.
const OP_INIT_TRANSFER_JETTON_FWD = 0x6e910020

// High bit marker on refund queryIds. Refunds (burn-refund from handleBurnBounce
// or handleBurnAborted) use OP_TEP74_TRANSFER just like legitimate LOCKED_NATIVE
// releases, so their bounces are indistinguishable by opcode alone. Namespacing
// the queryId by setting the top bit lets `onBouncedMessage` avoid clearing a
// destinationNonce bit when the bounce is actually a refund-of-refund, which
// would otherwise reopen an already-finalised transfer for replay.
const REFUND_QUERY_BIT: uint64 = 0x8000000000000000

struct JettonRecord {
    kind: uint8
    lockerJw: address
    decimals: uint8
}

struct JettonShelf {
    jettons: map<address, JettonRecord>
    masterByLockerJw: map<address, address>
    jettonMasterCode: cell
    jettonWalletCode: cell
}

// Keyed by master: both the TEP-89 reply and its bounce carry `senderAddress == master`.
struct PendingRegistration {
    caller: address
}

struct Storage {
    admin: address
    pendingAdmin: address?
    pauseFlags: uint8
    nearBridgeDerivedAddr: uint160
    chainId: uint8
    currentOriginNonce: uint64
    completedTransfers: map<uint64, uint256>
    shelf: Cell<JettonShelf>
    pendingRegistration: map<address, cell> // master → PendingRegistration cell
    pendingBurns: map<uint64, cell> // burn queryId → PendingBurn cell
    burnQueryCounter: uint64
}

// Stashed while a BRIDGE_MINTED burn is in flight: on success (BurnCompleteMsg
// from the master) we emit InitTransferEvent; on bounce of the burn op we
// refund the user.
struct PendingBurn {
    user: address
    master: address
    amount: uint128
    fee: uint128
    nativeFee: uint128
    recipientRef: cell
    messageRef: cell
}

fun Storage.load() {
    return Storage.fromCell(contract.getData());
}

fun Storage.save(self) {
    contract.setData(self.toCell());
}

struct (0x6e910002) FinTransferMsg {
    _queryId: uint64
    sigR: uint256
    sigS: uint256
    // y-parity ∈ {0,1}; relayer subtracts 27 from the MPC's v byte before submitting
    sigV: uint8
    payloadRef: cell
}

struct (0x6e910001) InitTransferNativeMsg {
    _queryId: uint64
    amount: uint128
    fee: uint128
    nativeFee: uint128
    recipientRef: cell
    messageRef: cell
}

struct (0x6e910003) DeployTokenMsg {
    _queryId: uint64
    sigR: uint256
    sigS: uint256
    sigV: uint8
    payloadRef: cell // Borsh-encoded MetadataPayload
    contentRef: cell // TEP-64 content cell for the jetton master
}

struct (0x6e910004) RegisterJettonMsg {
    _queryId: uint64
    kind: uint8 // JETTON_BRIDGE_MINTED or JETTON_LOCKED_NATIVE
    master: address
    lockerJw: address
    decimals: uint8
}

struct (0x6e910005) LogMetadataMsg {
    queryId: uint64
    master: address
}

struct (0x6e910010) SetPauseMsg {
    _queryId: uint64
    flags: uint8
}

struct (0x6e910012) SetAdminMsg {
    _queryId: uint64
    newAdmin: address
}

struct (0x6e910013) AcceptAdminMsg {
    _queryId: uint64
}

struct (0x6e910014) UpgradeCodeMsg {
    _queryId: uint64
    newCode: cell
}

struct (0x6e910015) SetJettonCodeMsg {
    _queryId: uint64
    newJettonMasterCode: cell
    newJettonWalletCode: cell
}

// Sent by the bridge-jetton master to the locker after processing a
// burn_notification from the locker's own JW. `queryId` is the burn queryId
// we allocated in handleTransferNotification; `amount` is the actually-burned
// amount (may differ from the stashed amount if master ever supports partial
// burns — currently always equals the stashed amount).
struct (0x6e910030) BurnCompleteMsg {
    queryId: uint64
    amount: uint128
}

// Sent by locker's own JW when burn_notification to master bounced and the
// JW re-credited its balance. Triggers: clear pending entry + refund user
// via TEP-74 transfer from the JW back to the original sender.
struct (0x6e910031) BurnAbortedMsg {
    queryId: uint64
    amount: uint128
}

// transfer_notification (op 0x7362d09c) is parsed manually in
// handleTransferNotification — see the comment there. Kept out of the
// struct/InMsg machinery so we can accept both Either-inline and Either-ref
// forward_payload variants per TEP-74.

struct (0xd1735400) TakeWalletAddressMsg {
    _queryId: uint64
    walletAddress: address
    _ownerAddrRef: cell? // TEP-89 `(Maybe ^MsgAddress)`, unused
}

type InMsg =
    | FinTransferMsg
    | InitTransferNativeMsg
    | DeployTokenMsg
    | RegisterJettonMsg
    | LogMetadataMsg
    | TakeWalletAddressMsg
    | SetPauseMsg
    | SetAdminMsg
    | AcceptAdminMsg
    | UpgradeCodeMsg
    | SetJettonCodeMsg
    | BurnCompleteMsg
    | BurnAbortedMsg

struct (0x99000001) InitTransferEvent {
    sender: address
    tokenMaster: address // addr_none for native TON
    originNonce: uint64
    amount: uint128
    fee: uint128
    nativeFee: uint128
    recipientRef: cell
    messageRef: cell
}

struct (0x99000002) FinTransferEvent {
    originChain: uint8
    originNonce: uint64
    destinationNonce: uint64
    recipient: uint256
    amount: uint128
    feeRecipientRef: cell // empty cell if None
    messageRef: cell // empty cell if empty
}

struct (0x99000003) DeployTokenEvent {
    master: address
    lockerJw: address
    decimals: uint8
    nearTokenIdRef: cell
}

struct (0x99000004) LogMetadataEvent {
    master: address
}

struct (0x99000006) RegisterJettonEvent {
    master: address
    lockerJw: address
    kind: uint8
}

struct (0x99000010) PauseStateChangedEvent {
    oldFlags: uint8
    newFlags: uint8
}

struct (0x99000011) AdminTransferredEvent {
    oldAdmin: address
    newAdmin: address
}

// destinationNonce = 0 when the bounced send had no body (e.g., native-TON release);
// operator correlates via `bouncedFrom` in that case.
struct (0x99000020) FinTransferStuckEvent {
    destinationNonce: uint64
    bouncedFrom: address
}

struct (0x99000022) BridgeMintedRefundedEvent {
    user: address
    master: address
    amount: uint128
    queryId: uint64
}

struct (0x99000023) DeployTokenFailedEvent {
    master: address
    lockerJw: address
}

struct (0x99000024) LogMetadataFailedEvent {
    master: address
}

// Byte-reversal via tree-of-swaps (cf. `evm/src/common/Borsh.sol`). The final
// stage needs an explicit high-mask because Tolk's `int` is arbitrary-precision
// and won't truncate the shift-left on its own.
@pure
fun swapBytes4(v: int): int {
    val x1 = ((v & 0x00ff00ff) << 8) | ((v & 0xff00ff00) >> 8);
    return ((x1 & 0x0000ffff) << 16) | ((x1 & 0xffff0000) >> 16);
}

@pure
fun swapBytes8(v: int): int {
    val x1 = ((v & 0x00ff00ff00ff00ff) << 8) | ((v & 0xff00ff00ff00ff00) >> 8);
    val x2 = ((x1 & 0x0000ffff0000ffff) << 16) | ((x1 & 0xffff0000ffff0000) >> 16);
    return ((x2 & 0xffffffff) << 32) | ((x2 & 0xffffffff00000000) >> 32);
}

@pure
fun swapBytes16(v: int): int {
    val x1 = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8) |
    ((v & 0xff00ff00ff00ff00ff00ff00ff00ff00) >> 8);
    val x2 = ((x1 & 0x0000ffff0000ffff0000ffff0000ffff) << 16) |
    ((x1 & 0xffff0000ffff0000ffff0000ffff0000) >> 16);
    val x3 = ((x2 & 0x00000000ffffffff00000000ffffffff) << 32) |
    ((x2 & 0xffffffff00000000ffffffff00000000) >> 32);
    return ((x3 & 0xffffffffffffffff) << 64) | ((x3 & 0xffffffffffffffff0000000000000000) >> 64);
}

fun loadBorshUint32LE(mutate s: slice): uint32 {
    return swapBytes4(s.loadUint(32)) as uint32;
}

fun loadBorshUint64LE(mutate s: slice): uint64 {
    return swapBytes8(s.loadUint(64)) as uint64;
}

fun loadBorshUint128LE(mutate s: slice): uint128 {
    return swapBytes16(s.loadUint(128)) as uint128;
}

fun verifyMpcSignature(
    payloadCell: cell,
    sigR: uint256,
    sigS: uint256,
    sigV: uint8,
    expected20: uint160,
) {
    assert (sigV <= 1) throw ERR_BAD_V;

    val msgHash = keccak256Slice(payloadCell.beginParse());

    val (_, pubX, pubY, ok) = tvmEcrecover(msgHash, sigV, sigR, sigS);
    assert (ok == -1) throw ERR_ECDSA_FAIL;

    val pubBytes = beginCell().storeUint(pubX, 256).storeUint(pubY, 256).endCell().beginParse();
    val recovered20 = keccak256Slice(pubBytes) & ((1 << 160) - 1);
    assert (recovered20 == expected20) throw ERR_BAD_SIGNER;
}

fun bitmapHasNonce(m: map<uint64, uint256>, nonce: uint64): bool {
    val slot = nonce >> 8;
    val bit = (1 as uint256) << (nonce & 0xff);
    val r = m.get(slot);
    if (r.isFound) {
        return (r.loadValue() & bit) != 0;
    }
    return false;
}

fun bitmapSetNonce(mutate m: map<uint64, uint256>, nonce: uint64): void {
    val slot = nonce >> 8;
    val bit = (1 as uint256) << (nonce & 0xff);
    val r = m.get(slot);
    val existing: uint256 = r.isFound ? r.loadValue() : 0 as uint256;
    m.set(slot, existing | bit);
}

// Used to roll back a marked-finalised nonce when the downstream release
// bounces — lets the relayer retry with the same signed payload.
fun bitmapClearNonce(mutate m: map<uint64, uint256>, nonce: uint64): void {
    val slot = nonce >> 8;
    val bit = (1 as uint256) << (nonce & 0xff);
    val r = m.get(slot);
    if (!r.isFound) {
        return;
    }
    val existing = r.loadValue();
    // If bit is set in `existing`, subtracting it clears that bit without
    // borrowing from higher positions. If bit isn't set, (existing & bit)
    // is 0 and the subtraction is a no-op. Avoids Tolk's arbitrary-precision
    // `~` semantics when operating on uint256.
    val cleared = existing - (existing & bit);
    if (cleared == 0) {
        m.delete(slot);
    } else {
        m.set(slot, cleared);
    }
}

fun onInternalMessage(in: InMessage) {
    val sender: address = in.senderAddress;
    val value: coins = in.valueCoins;

    // Peek the op before struct-matching. transfer_notification is handled
    // outside the auto-dispatch because its forward_payload field uses TEP-74
    // `Either Cell ^Cell` encoding. Tolk's struct `cell` field only loads a
    // ref; a standard jetton wallet sending Either=left$0 + inline data
    // would underflow the struct deserializer (exit 9) and the bounce would
    // return jettons to a user who has already lost their NEAR-side
    // allowance. Manual parsing here handles both inline and ref forms.
    if (in.body.remainingBitsCount() >= 32 && in.body.preloadUint(32) == 0x7362d09c) {
        handleTransferNotification(sender, in.body);
        return;
    }

    val msg = lazy InMsg.fromSlice(in.body);

    match (msg) {
        FinTransferMsg => handleFinTransfer(msg),
        InitTransferNativeMsg => handleInitTransferNative(sender, value, msg),
        DeployTokenMsg => handleDeployToken(msg),
        RegisterJettonMsg => handleRegisterJetton(sender, msg),
        LogMetadataMsg => handleLogMetadata(sender, value, msg),
        TakeWalletAddressMsg => handleTakeWalletAddress(sender, msg),
        SetPauseMsg => handleSetPause(sender, msg),
        SetAdminMsg => handleSetAdmin(sender, msg),
        AcceptAdminMsg => handleAcceptAdmin(sender),
        UpgradeCodeMsg => handleUpgradeCode(sender, msg),
        SetJettonCodeMsg => handleSetJettonCode(sender, msg),
        BurnCompleteMsg => handleBurnComplete(sender, msg),
        BurnAbortedMsg => handleBurnAborted(sender, msg),

        else => {
            assert (in.body.isEmpty()) throw ERR_WRONG_OP;
        }
    }
}

fun handleFinTransfer(msg: FinTransferMsg) {
    var st = lazy Storage.load();
    assert ((st.pauseFlags & PAUSE_FIN) == 0) throw ERR_PAUSED;

    verifyMpcSignature(msg.payloadRef, msg.sigR, msg.sigS, msg.sigV, st.nearBridgeDerivedAddr);

    var ps = msg.payloadRef.beginParse();

    val payloadType = ps.loadUint(8);
    assert (payloadType == PAYLOAD_TYPE_TRANSFER_MESSAGE) throw ERR_BAD_PAYLOAD_TYPE;

    val destinationNonce = loadBorshUint64LE(mutate ps);
    // Reserve the top bit for refund-queryId namespacing (see REFUND_QUERY_BIT).
    // A legitimate NEAR destinationNonce never approaches 2^63; this assertion
    // both prevents a compromised signer from poisoning the bit semantics and
    // guarantees that onBouncedMessage's refund-vs-release heuristic is sound.
    assert ((destinationNonce & REFUND_QUERY_BIT) == 0) throw ERR_BAD_PAYLOAD_TYPE;
    val originChain = ps.loadUint(8);
    val originNonce = loadBorshUint64LE(mutate ps);

    val destChainId1 = ps.loadUint(8);
    assert (destChainId1 == st.chainId) throw ERR_BAD_DESTINATION_CHAIN;

    val tokenAddr256 = ps.loadUint(256);
    val amount = loadBorshUint128LE(mutate ps);

    val destChainId2 = ps.loadUint(8);
    assert (destChainId2 == st.chainId) throw ERR_BAD_DESTINATION_CHAIN;

    val recipient256 = ps.loadUint(256);

    // Trailing Option<String> fee_recipient: 0x00 None | 0x01 + u32 len + utf8 bytes.
    val feeRecipientTag = ps.loadUint(8);
    val feeRecipientRef = feeRecipientTag == 0
        ? beginCell().endCell()
        : readBorshBytesToCell(mutate ps);
    // Trailing Option<bytes> message: no tag byte — presence is signaled by remaining bits.
    val messageRef = ps.remainingBitsCount() == 0 && ps.remainingRefsCount() == 0
        ? beginCell().endCell()
        : readBorshBytesToCell(mutate ps);

    // CEI: mark nonce used before any outgoing send.
    assert (!bitmapHasNonce(st.completedTransfers, destinationNonce)) throw ERR_NONCE_USED;
    bitmapSetNonce(mutate st.completedTransfers, destinationNonce);

    if (tokenAddr256 == 0) {
        st.save();
        dispatchNativeRelease(recipient256, amount);
    } else {
        val master = addressFromHash(0 as int8, tokenAddr256);
        val shelf = st.shelf.load();
        val recordLookup = shelf.jettons.get(master);
        assert (recordLookup.isFound) throw ERR_UNKNOWN_JETTON;
        val record = recordLookup.loadValue();
        st.save();
        dispatchJettonRelease(master, record, recipient256, amount, destinationNonce);
    }

    createExternalLogMessage({
        dest: createAddressNone(),
        body: FinTransferEvent {
            originChain,
            originNonce,
            destinationNonce,
            recipient: recipient256,
            amount,
            feeRecipientRef,
            messageRef,
        },
    })
        .send(SEND_MODE_REGULAR);
}

fun addressFromHash(workchain: int8, hash: uint256): address {
    var s = beginCell().storeUint(4, 3).storeInt(workchain as int, 8).storeUint(hash, 256).endCell()
        .beginParse();
    return s.loadAddress();
}

fun readBorshBytesToCell(mutate s: slice): cell {
    // Single-cell cap: 1023 bits / 8 = 127 bytes. Reject larger lengths explicitly
    // so a malformed/crafted payload throws here instead of underflow-ing downstream.
    val len = loadBorshUint32LE(mutate s);
    assert (len <= 127) throw ERR_BAD_PAYLOAD_TYPE;
    return beginCell().storeSlice(s.loadBits((len as int) * 8)).endCell();
}

fun dispatchNativeRelease(recipient256: uint256, amount: uint128) {
    // No body → bounces surface with destinationNonce=0 (see onBouncedMessage).
    createMessage({
        bounce: BounceMode.Only256BitsOfBody,
        dest: (0 as int8, recipient256),
        value: amount as coins,
    })
        .send(SEND_MODE_PAY_FEES_SEPARATELY);
}

fun dispatchJettonRelease(
    master: address,
    record: JettonRecord,
    recipient256: uint256,
    amount: uint128,
    destinationNonce: uint64,
) {
    val recipient = addressFromHash(0 as int8, recipient256);

    if (record.kind == JETTON_BRIDGE_MINTED) {
        // queryId = destinationNonce so bounces can be correlated back.
        createMessage({
            bounce: BounceMode.Only256BitsOfBody,
            dest: master,
            value: ton("0.05"),
            body: (MintMsgOut {
                queryId: destinationNonce,
                toAddr: recipient,
                jettonAmount: amount as coins,
                forwardTonAmount: 0 as coins,
            }),
        })
            .send(SEND_MODE_REGULAR);
    } else {
        val transferBody = beginCell().storeUint(OP_TEP74_TRANSFER, 32).storeUint(
            destinationNonce,
            64
        )
            // queryId = dest_nonce
            .storeCoins(amount as coins)
            .storeAddress(recipient)
            .storeAddress(recipient) // response_dst
            .storeUint(0, 1) // custom_payload = null
            .storeCoins(0 as coins) // forward_ton
            .storeUint(0, 1) // forward_payload = empty
            .endCell();
        createMessage({
            bounce: BounceMode.Only256BitsOfBody,
            dest: record.lockerJw,
            value: ton("0.05"),
            body: transferBody,
        })
            .send(SEND_MODE_REGULAR);
    }
}

struct (0x642b7d07) MintMsgOut {
    queryId: uint64
    toAddr: address
    jettonAmount: coins
    forwardTonAmount: coins
}

fun handleInitTransferNative(sender: address, value: coins, msg: InitTransferNativeMsg) {
    var st = lazy Storage.load();
    assert ((st.pauseFlags & PAUSE_INIT) == 0) throw ERR_PAUSED;
    assert (msg.amount > 0) throw ERR_AMOUNT_ZERO;
    assert (msg.fee < msg.amount) throw ERR_FEE_GTE_AMOUNT;
    // Require enough margin over (amount + nativeFee) for the locker to cover
    // its own compute, event emission, and storage rent. Without this, a user
    // can call with `value == amount + nativeFee` and drain the locker's TON
    // by 1 gas fee per init_transfer until it freezes.
    assert (
        (value as int) >= ((msg.amount + msg.nativeFee + INIT_TRANSFER_NATIVE_GAS_MARGIN) as int)
    ) throw ERR_VALUE_TOO_LOW;

    // Read/write once, use the local copy afterward.
    val nonce = st.currentOriginNonce + 1;
    st.currentOriginNonce = nonce;
    st.save();

    createExternalLogMessage({
        dest: createAddressNone(),
        body: InitTransferEvent {
            sender,
            tokenMaster: addressFromHash(0 as int8, 0 as uint256),
            originNonce: nonce,
            amount: msg.amount,
            fee: msg.fee,
            nativeFee: msg.nativeFee,
            recipientRef: msg.recipientRef,
            messageRef: msg.messageRef,
        },
    })
        .send(SEND_MODE_REGULAR);
}

fun handleDeployToken(msg: DeployTokenMsg) {
    var st = lazy Storage.load();
    assert ((st.pauseFlags & PAUSE_DEPLOY) == 0) throw ERR_PAUSED;

    verifyMpcSignature(msg.payloadRef, msg.sigR, msg.sigS, msg.sigV, st.nearBridgeDerivedAddr);

    var ps = msg.payloadRef.beginParse();
    val payloadType = ps.loadUint(8);
    assert (payloadType == PAYLOAD_TYPE_METADATA) throw ERR_BAD_PAYLOAD_TYPE;

    val nearTokenIdRef = readBorshBytesToCell(mutate ps);
    val _nameRef = readBorshBytesToCell(mutate ps);
    val _symbolRef = readBorshBytesToCell(mutate ps);
    val decimals = ps.loadUint(8);

    var shelf = st.shelf.load();

    val masterState = ContractState {
        code: shelf.jettonMasterCode,
        data: (JettonMasterInitState {
            totalSupply: 0 as coins,
            adminAddr: contract.getAddress(),
            walletCode: shelf.jettonWalletCode,
            contentRef: msg.contentRef,
        })
            .toCell(),
    };

    val masterAutoAddr = AutoDeployAddress { workchain: 0, stateInit: masterState };
    val masterAddress = masterAutoAddr.calculateAddress();

    val lockerJwInit = ContractState {
        code: shelf.jettonWalletCode,
        data: (JettonWalletInitState {
            balance: 0 as coins,
            ownerAddr: contract.getAddress(),
            masterAddr: masterAddress,
            walletCode: shelf.jettonWalletCode,
        })
            .toCell(),
    };
    val lockerJw = (AutoDeployAddress { workchain: 0, stateInit: lockerJwInit }).calculateAddress();

    assert (!shelf.jettons.exists(masterAddress)) throw ERR_JETTON_ALREADY_REGISTERED;
    shelf.jettons.set(
        masterAddress,
        JettonRecord {
            kind: JETTON_BRIDGE_MINTED,
            lockerJw,
            decimals: decimals as uint8,
        }
    );
    shelf.masterByLockerJw.set(lockerJw, masterAddress);
    st.shelf = shelf.toCell();
    st.save();

    createMessage({
        bounce: true,
        dest: masterAutoAddr,
        value: ton("0.1"),
    })
        .send(SEND_MODE_REGULAR);

    createExternalLogMessage({
        dest: createAddressNone(),
        body: DeployTokenEvent {
            master: masterAddress,
            lockerJw,
            decimals: decimals as uint8,
            nearTokenIdRef,
        },
    })
        .send(SEND_MODE_REGULAR);
}

struct JettonMasterInitState {
    totalSupply: coins
    adminAddr: address
    walletCode: cell
    contentRef: cell
}

struct JettonWalletInitState {
    balance: coins
    ownerAddr: address
    masterAddr: address
    walletCode: cell
}

fun handleRegisterJetton(sender: address, msg: RegisterJettonMsg) {
    var st = lazy Storage.load();
    assert (sender == st.admin) throw ERR_NOT_ADMIN;
    assert ((st.pauseFlags & PAUSE_DEPLOY) == 0) throw ERR_PAUSED;
    assert (
        msg.kind == JETTON_BRIDGE_MINTED || msg.kind == JETTON_LOCKED_NATIVE
    ) throw ERR_BAD_FLAGS;
    var shelf = st.shelf.load();
    assert (!shelf.jettons.exists(msg.master)) throw ERR_JETTON_ALREADY_REGISTERED;
    // Anti-hijack: reject if the claimed lockerJw is already tied to a different
    // master. Without this, an attacker could later run TEP-89 against a fake
    // master and overwrite this reverse mapping, silently misrouting a legit
    // jetton's transfer_notification to the attacker's master.
    assert (!shelf.masterByLockerJw.exists(msg.lockerJw)) throw ERR_JETTON_ALREADY_REGISTERED;

    shelf.jettons.set(
        msg.master,
        JettonRecord {
            kind: msg.kind,
            lockerJw: msg.lockerJw,
            decimals: msg.decimals,
        }
    );
    shelf.masterByLockerJw.set(msg.lockerJw, msg.master);
    st.shelf = shelf.toCell();
    st.save();

    createExternalLogMessage({
        dest: createAddressNone(),
        body: RegisterJettonEvent {
            master: msg.master,
            lockerJw: msg.lockerJw,
            kind: msg.kind,
        },
    })
        .send(SEND_MODE_REGULAR);
}

// Kicks off a TEP-89 handshake with `master`; the reply lands in
// `handleTakeWalletAddress` which is what actually populates the map entry
// and emits `LogMetadataEvent`. `lockerJw` must come from the master itself
// (caller-provided would allow impersonating the reverse map).
fun handleLogMetadata(sender: address, value: coins, msg: LogMetadataMsg) {
    var st = lazy Storage.load();
    assert ((st.pauseFlags & PAUSE_DEPLOY) == 0) throw ERR_PAUSED;
    assert ((value as int) >= MIN_LOG_METADATA_FEE) throw ERR_REGISTRATION_FEE_LOW;

    var shelf = st.shelf.load();
    assert (!shelf.jettons.exists(msg.master)) throw ERR_JETTON_ALREADY_REGISTERED;
    assert (!st.pendingRegistration.exists(msg.master)) throw ERR_REGISTRATION_PENDING;

    st.pendingRegistration.set(msg.master, (PendingRegistration { caller: sender }).toCell());
    st.save();

    val provideBody = beginCell().storeUint(OP_PROVIDE_WALLET_ADDRESS, 32).storeUint(
        msg.queryId,
        64
    )
        .storeAddress(contract.getAddress()) // owner = locker
        .storeUint(0, 1) // include_address = false
        .endCell();

    createMessage({
        bounce: BounceMode.Only256BitsOfBody,
        dest: msg.master,
        value: ton("0.05"),
        body: provideBody,
    })
        .send(SEND_MODE_REGULAR);
}

// TEP-89 reply. Trust anchor: `sender == master`, which is how we match
// the pending entry. Stray replies are silently dropped.
fun handleTakeWalletAddress(sender: address, msg: TakeWalletAddressMsg) {
    var st = lazy Storage.load();
    val pendingLookup = st.pendingRegistration.get(sender);
    if (!pendingLookup.isFound) {
        return;
    }
    val pending = PendingRegistration.fromCell(pendingLookup.loadValue());

    var shelf = st.shelf.load();
    // Race with admin `register_jetton` completing first — drop our entry and exit.
    if (shelf.jettons.exists(sender)) {
        st.pendingRegistration.delete(sender);
        st.save();
        return;
    }

    // Anti-hijack: an attacker's fake master can reply with any walletAddress
    // including one that already backs a legitimate jetton. Reject if already
    // mapped, refunding the caller so grief costs nothing above gas.
    if (shelf.masterByLockerJw.exists(msg.walletAddress)) {
        st.pendingRegistration.delete(sender);
        st.save();
        createExternalLogMessage({
            dest: createAddressNone(),
            body: LogMetadataFailedEvent { master: sender },
        })
            .send(SEND_MODE_REGULAR);
        createMessage({
            bounce: false,
            dest: pending.caller,
            value: 0 as coins,
        })
            .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
        return;
    }

    shelf.jettons.set(
        sender,
        JettonRecord {
            kind: JETTON_LOCKED_NATIVE,
            lockerJw: msg.walletAddress,
            decimals: 0, // placeholder; DAO finalizes on NEAR
        }
    );
    shelf.masterByLockerJw.set(msg.walletAddress, sender);
    st.shelf = shelf.toCell();
    st.pendingRegistration.delete(sender);
    st.save();

    createExternalLogMessage({
        dest: createAddressNone(),
        body: LogMetadataEvent { master: sender },
    })
        .send(SEND_MODE_REGULAR);

    createMessage({
        bounce: false,
        dest: pending.caller,
        value: 0 as coins,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

fun handleSetPause(sender: address, msg: SetPauseMsg) {
    var st = lazy Storage.load();
    assert (sender == st.admin) throw ERR_NOT_ADMIN;
    assert ((msg.flags & (0xff - VALID_PAUSE_FLAGS)) == 0) throw ERR_BAD_FLAGS;
    val old = st.pauseFlags;
    st.pauseFlags = msg.flags;
    st.save();
    createExternalLogMessage({
        dest: createAddressNone(),
        body: PauseStateChangedEvent { oldFlags: old, newFlags: msg.flags },
    })
        .send(SEND_MODE_REGULAR);
}

fun handleSetAdmin(sender: address, msg: SetAdminMsg) {
    var st = lazy Storage.load();
    assert (sender == st.admin) throw ERR_NOT_ADMIN;
    st.pendingAdmin = msg.newAdmin;
    st.save();
}

fun handleAcceptAdmin(sender: address) {
    var st = lazy Storage.load();
    val pa = st.pendingAdmin;
    assert (pa != null && sender == pa) throw ERR_NOT_PENDING_ADMIN;
    val oldAdmin = st.admin;
    st.admin = pa;
    st.pendingAdmin = null;
    st.save();
    createExternalLogMessage({
        dest: createAddressNone(),
        body: AdminTransferredEvent { oldAdmin, newAdmin: pa },
    })
        .send(SEND_MODE_REGULAR);
}

// Sensitive operation: swaps out the locker's own code. Must be preceded by
// `set_pause` blocking both transfer directions so in-flight messages can't
// hit an inconsistent mix of old+new logic mid-upgrade. For stronger
// guarantees (timelock, propose+accept), admin should be a governance
// contract that enforces them off-chain.
fun handleUpgradeCode(sender: address, msg: UpgradeCodeMsg) {
    var st = lazy Storage.load();
    assert (sender == st.admin) throw ERR_NOT_ADMIN;
    // Require full-contract pause — any of INIT/FIN/DEPLOY in flight could
    // land post-setCodePostponed and hit new code with stale storage shape
    // (e.g. `pendingRegistration` replies landing under a schema change).
    assert (
        (st.pauseFlags & (PAUSE_INIT | PAUSE_FIN | PAUSE_DEPLOY))
            == (PAUSE_INIT | PAUSE_FIN | PAUSE_DEPLOY)
    ) throw ERR_NOT_PAUSED;
    contract.setCodePostponed(msg.newCode);
}

// Admin: replace the code cells used to deploy future bridge-jetton masters
// and their wallets. Existing deployed masters/wallets are unaffected — their
// code is immutable. Intended for iterating on the jetton contracts without
// redeploying the locker. Pause-gated so it can't race with in-flight
// deploy_token / fin_transfer dispatch that reads shelf.jettonMasterCode /
// jettonWalletCode.
fun handleSetJettonCode(sender: address, msg: SetJettonCodeMsg) {
    var st = lazy Storage.load();
    assert (sender == st.admin) throw ERR_NOT_ADMIN;
    assert ((st.pauseFlags & PAUSE_DEPLOY) != 0) throw ERR_NOT_PAUSED;
    var shelf = st.shelf.load();
    shelf.jettonMasterCode = msg.newJettonMasterCode;
    shelf.jettonWalletCode = msg.newJettonWalletCode;
    st.shelf = shelf.toCell();
    st.save();
}

// Manually parses the body to handle TEP-74's Either Cell ^Cell encoding for
// forward_payload. Tolk's struct `cell` field only loads a ref; a standard
// jetton wallet that sends forward_payload inline (Either=left$0) would
// underflow the struct deserializer and the locker would bounce the jettons
// back — losing them for the user (the NEAR-side allowance was already
// committed). This handler accepts both forms.
fun handleTransferNotification(sender: address, body: slice) {
    var st = lazy Storage.load();
    assert ((st.pauseFlags & PAUSE_INIT) == 0) throw ERR_PAUSED;

    var bs = body;
    bs.loadUint(32); // op (already matched)
    bs.loadUint(64); // queryId — unused
    val amount = bs.loadCoins();
    val fromAddr = bs.loadAddress();
    assert (amount > 0) throw ERR_AMOUNT_ZERO;

    // Impersonation defense: sender must be the locker's own JW for a registered master.
    val shelf = st.shelf.load();
    val masterLookup = shelf.masterByLockerJw.get(sender);
    assert (masterLookup.isFound) throw ERR_IMPERSONATED_JETTON;
    val master = masterLookup.loadValue();

    val recordLookup = shelf.jettons.get(master);
    assert (recordLookup.isFound) throw ERR_UNKNOWN_JETTON;
    val record = recordLookup.loadValue();

    // TEP-74 forward_payload: (Either Cell ^Cell). Flag 1 = ref; 0 = inline.
    assert (bs.remainingBitsCount() >= 1) throw ERR_BAD_FORWARD_PAYLOAD;
    val eitherFlag = bs.loadUint(1);
    var fp = eitherFlag == 1
        ? bs.loadRef().beginParse()
        : bs;

    assert (fp.remainingBitsCount() >= 32) throw ERR_BAD_FORWARD_PAYLOAD;
    val fwdOp = fp.loadUint(32);
    assert (fwdOp == OP_INIT_TRANSFER_JETTON_FWD) throw ERR_BAD_FORWARD_PAYLOAD;

    val fee = fp.remainingBitsCount() >= 128 ? fp.loadUint(128) : 0;
    val nativeFee = fp.remainingBitsCount() >= 128 ? fp.loadUint(128) : 0;
    val recipientCell = fp.remainingRefsCount() > 0 ? fp.loadRef() : beginCell().endCell();
    val messageCell = fp.remainingRefsCount() > 0 ? fp.loadRef() : beginCell().endCell();
    // Match handleInitTransferNative: fee must leave a positive release
    // amount. Reject before we allocate nonces / emit events.
    assert ((fee as int) < (amount as int)) throw ERR_FEE_GTE_AMOUNT;

    if (record.kind == JETTON_BRIDGE_MINTED) {
        // Stash state, send burn to our own JW. Emit InitTransferEvent only
        // once the master's burn_notification round-trip resolves (see
        // handleBurnComplete). On bounce of the burn we refund the user.
        val burnQueryId = st.burnQueryCounter + 1;
        st.burnQueryCounter = burnQueryId;
        val pending = (PendingBurn {
            user: fromAddr,
            master,
            amount: amount as uint128,
            fee: fee as uint128,
            nativeFee: nativeFee as uint128,
            recipientRef: recipientCell,
            messageRef: messageCell,
        })
            .toCell();
        st.pendingBurns.set(burnQueryId, pending);
        st.save();

        val burnBody = beginCell().storeUint(OP_TEP74_BURN, 32).storeUint(burnQueryId, 64)
            .storeCoins(amount)
            .storeAddress(contract.getAddress()) // response_destination = locker
            .storeUint(0, 1) // custom_payload = null
            .endCell();
        createMessage({
            bounce: BounceMode.Only256BitsOfBody,
            dest: record.lockerJw,
            value: ton("0.05"),
            body: burnBody,
        })
            .send(SEND_MODE_REGULAR);
        return;
    }

    val nonce = st.currentOriginNonce + 1;
    st.currentOriginNonce = nonce;
    st.save();

    createExternalLogMessage({
        dest: createAddressNone(),
        body: InitTransferEvent {
            sender: fromAddr,
            tokenMaster: master,
            originNonce: nonce,
            amount: amount as uint128,
            fee: fee as uint128,
            nativeFee: nativeFee as uint128,
            recipientRef: recipientCell,
            messageRef: messageCell,
        },
    })
        .send(SEND_MODE_REGULAR);
}

// Round-trip closer for BRIDGE_MINTED outbound: master has processed
// burn_notification and confirmed the burn. We emit InitTransferEvent now
// (never before — emitting before burn is a double-spend vector).
fun handleBurnComplete(sender: address, msg: BurnCompleteMsg) {
    // Bounce-proof: if this handler throws, the message bounces back to the
    // master which silently drops it. That would leave the user's tokens
    // burned on both the JW and the master but with no InitTransferEvent on
    // NEAR — permanent loss. Instead, fail-silent on unexpected inputs.
    var st = lazy Storage.load();
    val pendingLookup = st.pendingBurns.get(msg.queryId);
    if (!pendingLookup.isFound) {
        return;
    }
    val pending = PendingBurn.fromCell(pendingLookup.loadValue());
    // Silently ignore spoofed completions and amount mismatches; do NOT emit
    // or clear state.
    if (sender != pending.master || msg.amount != pending.amount) {
        return;
    }

    val nonce = st.currentOriginNonce + 1;
    st.currentOriginNonce = nonce;
    st.pendingBurns.delete(msg.queryId);
    st.save();

    createExternalLogMessage({
        dest: createAddressNone(),
        body: InitTransferEvent {
            sender: pending.user,
            tokenMaster: pending.master,
            originNonce: nonce,
            amount: pending.amount,
            fee: pending.fee,
            nativeFee: pending.nativeFee,
            recipientRef: pending.recipientRef,
            messageRef: pending.messageRef,
        },
    })
        .send(SEND_MODE_REGULAR);
}

// Cleanup for BRIDGE_MINTED outbound that failed at the master hop. Wallet
// has already re-credited its balance; we just need to refund the user via
// TEP-74 transfer from the JW. Bounce-proof: silent return on unknown
// queryId / wrong sender keeps the wallet's re-credit without throwing.
fun handleBurnAborted(sender: address, msg: BurnAbortedMsg) {
    var st = lazy Storage.load();
    val pendingLookup = st.pendingBurns.get(msg.queryId);
    if (!pendingLookup.isFound) {
        return;
    }
    val pending = PendingBurn.fromCell(pendingLookup.loadValue());
    val shelf = st.shelf.load();
    val recordLookup = shelf.jettons.get(pending.master);
    if (!recordLookup.isFound) {
        return;
    }
    val record = recordLookup.loadValue();
    // Only accept the abort signal from the master's own locker JW.
    if (sender != record.lockerJw) {
        return;
    }
    // Defense-in-depth: the JW should always echo the same amount it
    // re-credited itself for. A mismatch implies a future wallet-code drift
    // and we should leave the pending entry for manual resolution rather
    // than double-refund on an inconsistent signal.
    if (msg.amount != pending.amount) {
        return;
    }
    st.pendingBurns.delete(msg.queryId);
    st.save();

    // Mark the refund queryId so onBouncedMessage can distinguish it from a
    // release-bounce and skip the nonce-clear (which would otherwise reopen
    // an unrelated finalised transfer for replay).
    val refundQueryId = (msg.queryId as uint64) | REFUND_QUERY_BIT;
    val refundBody = beginCell().storeUint(OP_TEP74_TRANSFER, 32).storeUint(refundQueryId, 64)
        .storeCoins(pending.amount as coins)
        .storeAddress(pending.user) // destination
        .storeAddress(pending.user) // response_dst
        .storeUint(0, 1) // custom_payload = null
        .storeCoins(0 as coins) // forward_ton = 0
        .storeUint(0, 1) // forward_payload = empty inline
        .endCell();
    createMessage({
        bounce: BounceMode.Only256BitsOfBody,
        dest: record.lockerJw,
        value: ton("0.05"),
        body: refundBody,
    })
        .send(SEND_MODE_REGULAR);

    createExternalLogMessage({
        dest: createAddressNone(),
        body: BridgeMintedRefundedEvent {
            user: pending.user,
            master: pending.master,
            amount: pending.amount,
            queryId: msg.queryId,
        },
    })
        .send(SEND_MODE_REGULAR);
}

fun onBouncedMessage(in: InMessageBounced) {
    // Bounce dispatch:
    //  op == OP_PROVIDE_WALLET_ADDRESS → TEP-89 handshake failure
    //  op + queryId in body            → mint/TEP-74/refund stuck
    //  empty body, sender ∈ jettons    → deploy_token deploy failed, revert entry
    //  empty body, sender unknown      → native-TON release bounce
    var body = in.bouncedBody;
    body.skipBouncedPrefix();

    if (body.remainingBitsCount() >= 32) {
        val op = body.loadUint(32);

        if (op == OP_PROVIDE_WALLET_ADDRESS) {
            handleProvideWalletAddressBounce(in.senderAddress);
            return;
        }

        if (op == OP_TEP74_BURN) {
            handleBurnBounce(in.senderAddress, mutate body);
            return;
        }

        // Releases we issued in fin_transfer (MintMsgOut = 0x642b7d07 for
        // BRIDGE_MINTED, OP_TEP74_TRANSFER for LOCKED_NATIVE) use
        // queryId == destinationNonce. If such a release bounces, we clear
        // the nonce bit so the relayer can retry — without this the nonce
        // is forever burned and the user loses funds on both chains.
        val isReleaseBounce = (op == 0x642b7d07) || (op == OP_TEP74_TRANSFER);
        if (body.remainingBitsCount() >= 64) {
            val queryIdRaw = body.loadUint(64) as uint64;
            // Burn refunds (handleBurnBounce / handleBurnAborted) also use
            // OP_TEP74_TRANSFER but tag their queryId with REFUND_QUERY_BIT.
            // A bounce of a refund must NOT clear a destinationNonce — that
            // would reopen an unrelated finalised transfer for replay.
            val isRefundBounce = (queryIdRaw & REFUND_QUERY_BIT) != 0;
            // Explicit 63-bit mask (rather than `~REFUND_QUERY_BIT`) — Tolk's
            // arbitrary-precision `~` would produce a negative-typed value
            // whose AND semantics with uint64 depend on internal extension
            // rules; literal mask avoids that ambiguity.
            val destinationNonce = queryIdRaw & 0x7fffffffffffffff;
            if (isReleaseBounce && !isRefundBounce) {
                var st = lazy Storage.load();
                if (bitmapHasNonce(st.completedTransfers, destinationNonce)) {
                    bitmapClearNonce(mutate st.completedTransfers, destinationNonce);
                    st.save();
                }
            }
            createExternalLogMessage({
                dest: createAddressNone(),
                body: FinTransferStuckEvent { destinationNonce, bouncedFrom: in.senderAddress },
            })
                .send(SEND_MODE_REGULAR);
            return;
        }
    }

    var st = lazy Storage.load();
    var shelf = st.shelf.load();
    val masterLookup = shelf.jettons.get(in.senderAddress);
    if (masterLookup.isFound) {
        val record = masterLookup.loadValue();
        shelf.jettons.delete(in.senderAddress);
        shelf.masterByLockerJw.delete(record.lockerJw);
        st.shelf = shelf.toCell();
        st.save();
        createExternalLogMessage({
            dest: createAddressNone(),
            body: DeployTokenFailedEvent { master: in.senderAddress, lockerJw: record.lockerJw },
        })
            .send(SEND_MODE_REGULAR);
        return;
    }

    createExternalLogMessage({
        dest: createAddressNone(),
        body: FinTransferStuckEvent { destinationNonce: 0 as uint64, bouncedFrom: in.senderAddress },
    })
        .send(SEND_MODE_REGULAR);
}

// Burn op bounced back from locker's own JW (its compute threw → balance is
// untouched). Refund the user by sending the jettons back out via TEP-74
// transfer from the locker's JW, and clear the pending entry.
fun handleBurnBounce(from: address, mutate body: slice) {
    if (body.remainingBitsCount() < 64) {
        return;
    }
    val queryId = body.loadUint(64) as uint64;

    var st = lazy Storage.load();
    val pendingLookup = st.pendingBurns.get(queryId);
    if (!pendingLookup.isFound) {
        return;
    }
    val pending = PendingBurn.fromCell(pendingLookup.loadValue());
    st.pendingBurns.delete(queryId);
    st.save();

    // See REFUND_QUERY_BIT: mark as refund so a bounce of this refund doesn't
    // clear an unrelated finalised destinationNonce.
    val refundQueryId = queryId | REFUND_QUERY_BIT;
    val transferBody = beginCell().storeUint(OP_TEP74_TRANSFER, 32).storeUint(refundQueryId, 64)
        .storeCoins(pending.amount as coins)
        .storeAddress(pending.user) // destination
        .storeAddress(pending.user) // response_dst
        .storeUint(0, 1) // custom_payload = null
        .storeCoins(0 as coins) // forward_ton = 0
        .storeUint(0, 1) // forward_payload = empty inline
        .endCell();
    createMessage({
        bounce: BounceMode.Only256BitsOfBody,
        dest: from,
        value: ton("0.05"),
        body: transferBody,
    })
        .send(SEND_MODE_REGULAR);

    createExternalLogMessage({
        dest: createAddressNone(),
        body: BridgeMintedRefundedEvent {
            user: pending.user,
            master: pending.master,
            amount: pending.amount,
            queryId,
        },
    })
        .send(SEND_MODE_REGULAR);
}

fun handleProvideWalletAddressBounce(master: address) {
    var st = lazy Storage.load();
    val pendingLookup = st.pendingRegistration.get(master);
    if (!pendingLookup.isFound) {
        return;
    }
    val pending = PendingRegistration.fromCell(pendingLookup.loadValue());
    st.pendingRegistration.delete(master);
    st.save();

    createExternalLogMessage({ dest: createAddressNone(), body: LogMetadataFailedEvent { master } })
        .send(SEND_MODE_REGULAR);

    createMessage({
        bounce: false,
        dest: pending.caller,
        value: 0 as coins,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

get fun getState(): (int, int, int, int) {
    val st = lazy Storage.load();
    return (st.nearBridgeDerivedAddr, st.chainId, st.currentOriginNonce, st.pauseFlags);
}

get fun getAdmin(): (address, address?) {
    val st = lazy Storage.load();
    return (st.admin, st.pendingAdmin);
}

get fun isTransferFinalised(destinationNonce: int): bool {
    val st = lazy Storage.load();
    return bitmapHasNonce(st.completedTransfers, destinationNonce as uint64);
}

get fun getJetton(master: address): (bool, int, address, int) {
    val st = lazy Storage.load();
    val shelf = st.shelf.load();
    val r = shelf.jettons.get(master);
    if (r.isFound) {
        val rec = r.loadValue();
        return (true, rec.kind, rec.lockerJw, rec.decimals);
    }
    return (false, 0, addressFromHash(0 as int8, 0 as uint256), 0);
}

get fun getPendingRegistration(master: address): (bool, address) {
    val st = lazy Storage.load();
    val r = st.pendingRegistration.get(master);
    if (r.isFound) {
        val p = PendingRegistration.fromCell(r.loadValue());
        return (true, p.caller);
    }
    return (false, addressFromHash(0 as int8, 0 as uint256));
}
