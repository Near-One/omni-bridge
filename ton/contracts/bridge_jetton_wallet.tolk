tolk 1.3

// TEP-74 jetton wallet for bridge-minted jettons. Handles:
//   - transfer (from owner) → debit + deploy-or-send InternalTransfer to peer
//   - internal_transfer (from master or peer) → credit + optional transfer_notification
//   - burn (from owner) → debit + burn_notification to master (closes NEAR round-trip)
const OP_INTERNAL_TRANSFER = 0x178d4519
const OP_TRANSFER = 0x0f8a7ea5
const OP_TRANSFER_NOTIFICATION = 0x7362d09c
const OP_EXCESSES = 0xd53276db
const OP_BURN = 0x595f07bc
const OP_BURN_NOTIFICATION = 0x7bdd97de
const OP_BURN_ABORTED = 0x6e910031

// `SEND_MODE_PAY_FEES_SEPARATELY + SEND_MODE_IGNORE_ERRORS`. Forward fees
// come from the contract's balance rather than the attached `value`, and any
// action-phase failure (e.g. destination deploy short of rent) is dropped
// silently instead of aborting the whole tx.
const SEND_MODE_PAY_GAS_AND_IGNORE_ERRORS = 3
// `SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE + SEND_MODE_IGNORE_ERRORS`.
// Used for excesses to reliably forward whatever is left after the
// notification pays its own gas.
const SEND_MODE_REMAINING_AND_IGNORE_ERRORS = 66

const ERR_NOT_FROM_MASTER_OR_PEER = 401
const ERR_NOT_FROM_OWNER = 402
const ERR_INSUFFICIENT_BALANCE = 403

struct Storage {
    balance: coins
    ownerAddr: address
    masterAddr: address
    walletCode: cell
}

fun Storage.load() {
    return Storage.fromCell(contract.getData());
}

fun Storage.save(self) {
    contract.setData(self.toCell());
}

fun onInternalMessage(in: InMessage) {
    val op = in.body.remainingBitsCount() >= 32 ? in.body.preloadUint(32) : 0;

    if (op == OP_TRANSFER) {
        handleTransfer(in.senderAddress, in.body);
        return;
    }
    if (op == OP_INTERNAL_TRANSFER) {
        handleInternalTransfer(in.senderAddress, in.body);
        return;
    }
    if (op == OP_BURN) {
        handleBurn(in.senderAddress, in.body);
        return;
    }
}

// TEP-74 transfer from owner. Debit, deploy-or-send InternalTransfer to the
// recipient's JW. This kicks off the user→user-JW→locker-JW cascade that the
// locker's handleTransferNotification needs to see.
fun handleTransfer(sender: address, body: slice) {
    var st = lazy Storage.load();
    assert (sender == st.ownerAddr) throw ERR_NOT_FROM_OWNER;

    var bs = body;
    bs.loadUint(32); // op
    val queryId = bs.loadUint(64) as uint64;
    val amount = bs.loadCoins();
    val destOwner = bs.loadAddress();
    val responseDst = bs.loadAddress();
    val hasCustom = bs.loadUint(1);
    if (hasCustom == 1) {
        bs.loadRef();
    }
    val forwardTon = bs.loadCoins();
    // forward_payload: (Either Cell ^Cell). Unwrap the Either flag so fpCell
    // is the actual inner payload — not a double-wrapped cell-containing-flag.
    val fpEitherFlag = bs.loadUint(1);
    val fpCell = fpEitherFlag == 1 ? bs.loadRef() : beginCell().storeSlice(bs).endCell();

    assert (st.balance >= amount) throw ERR_INSUFFICIENT_BALANCE;
    st.balance = st.balance - amount;
    st.save();

    val peerInit = ContractState {
        code: st.walletCode,
        data: (Storage {
            balance: 0 as coins,
            ownerAddr: destOwner,
            masterAddr: st.masterAddr,
            walletCode: st.walletCode,
        })
            .toCell(),
    };

    val internalBody = beginCell().storeUint(OP_INTERNAL_TRANSFER, 32).storeUint(queryId, 64)
        .storeCoins(amount)
        .storeAddress(st.ownerAddr) // from
        .storeAddress(responseDst)
        .storeCoins(forwardTon)
        .storeUint(1, 1) // Maybe(ref): Some
        .storeRef(fpCell)
        .endCell();

    createMessage({
        bounce: true,
        dest: AutoDeployAddress { workchain: 0, stateInit: peerInit },
        value: 0 as coins,
        body: internalBody,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

// Incoming InternalTransfer: from our master (mint) or a peer wallet (inbound
// transfer). Credit, then relay transfer_notification to owner when
// forward_ton > 0, otherwise return excesses to responseDst.
fun handleInternalTransfer(sender: address, body: slice) {
    var st = lazy Storage.load();

    var bs = body;
    bs.loadUint(32); // op
    val queryId = bs.loadUint(64) as uint64;
    val amount = bs.loadCoins();
    val fromAddr = bs.loadAddress();
    val responseDst = bs.loadAddress();
    val forwardTon = bs.loadCoins();
    val hasFp = bs.loadUint(1);
    val fpCell = hasFp == 1 ? bs.loadRef() : beginCell().endCell();

    if (sender != st.masterAddr) {
        val expectedPeer = (AutoDeployAddress {
            workchain: 0,
            stateInit: ContractState {
                code: st.walletCode,
                data: (Storage {
                    balance: 0 as coins,
                    ownerAddr: fromAddr,
                    masterAddr: st.masterAddr,
                    walletCode: st.walletCode,
                })
                    .toCell(),
            },
        })
            .calculateAddress();
        assert (sender == expectedPeer) throw ERR_NOT_FROM_MASTER_OR_PEER;
    }

    st.balance = st.balance + amount;
    st.save();

    // Standard TEP-74 two-send pattern, both sends use IGNORE_ERRORS so an
    // action-phase failure on either one (e.g. tight value at first-receive)
    // just drops the action instead of aborting the transaction and bouncing
    // the jettons back through the chain.
    if (forwardTon > 0) {
        val notifBody = beginCell().storeUint(OP_TRANSFER_NOTIFICATION, 32)
            .storeUint(queryId, 64)
            .storeCoins(amount)
            .storeAddress(fromAddr)
            .storeUint(1, 1) // Either = right$1 — forward_payload as ref
            .storeRef(fpCell)
            .endCell();
        createMessage({
            bounce: false,
            dest: st.ownerAddr,
            value: forwardTon,
            body: notifBody,
        })
            .send(SEND_MODE_PAY_GAS_AND_IGNORE_ERRORS);
    }

    // Excesses: return unused incoming value to responseDst. If the sender
    // wrote addr_none (meaning "don't care"), IGNORE_ERRORS drops the send
    // silently and the TON accrues to this wallet's rent buffer — safe
    // fallback without the complexity of custom addr_none detection in Tolk.
    val excessesBody = beginCell().storeUint(OP_EXCESSES, 32).storeUint(queryId, 64).endCell();
    createMessage({
        bounce: false,
        dest: responseDst,
        value: 0 as coins,
        body: excessesBody,
    })
        .send(SEND_MODE_REMAINING_AND_IGNORE_ERRORS);
}

fun handleBurn(sender: address, body: slice) {
    var st = lazy Storage.load();
    assert (sender == st.ownerAddr) throw ERR_NOT_FROM_OWNER;

    var bs = body;
    bs.loadUint(32); // op
    val queryId = bs.loadUint(64) as uint64;
    val amount = bs.loadCoins();
    val responseDst = bs.loadAddress();

    assert (st.balance >= amount) throw ERR_INSUFFICIENT_BALANCE;
    st.balance = st.balance - amount;
    st.save();

    val notifBody = beginCell().storeUint(OP_BURN_NOTIFICATION, 32).storeUint(queryId, 64)
        .storeCoins(amount)
        .storeAddress(st.ownerAddr)
        .storeAddress(responseDst)
        .endCell();
    createMessage({
        bounce: true,
        dest: st.masterAddr,
        value: 0 as coins,
        body: notifBody,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

fun onBouncedMessage(in: InMessageBounced) {
    var body = in.bouncedBody;
    body.skipBouncedPrefix();
    if (body.remainingBitsCount() < 32 + 64) {
        return;
    }
    val op = body.loadUint(32);
    if (op == OP_BURN_NOTIFICATION) {
        val queryId = body.loadUint(64) as uint64;
        val amount = body.loadCoins();
        var st = lazy Storage.load();
        st.balance = st.balance + amount;
        st.save();
        // Tell the owner (locker) the burn was aborted so it can clear the
        // PendingBurn entry and refund the user. Fire-and-forget — if the
        // locker is down, the wallet still has the re-credited balance and
        // admin can trigger a manual refund.
        val abortedBody = beginCell().storeUint(OP_BURN_ABORTED, 32)
            .storeUint(queryId, 64)
            .storeUint(amount as uint128, 128)
            .endCell();
        createMessage({
            bounce: false,
            dest: st.ownerAddr,
            value: 0 as coins,
            body: abortedBody,
        })
            .send(SEND_MODE_REMAINING_AND_IGNORE_ERRORS);
        return;
    }
    // Peer InternalTransfer bounced (peer JW compute failed). Re-credit ours so
    // the user's debit atomically rolls back.
    if (op == OP_INTERNAL_TRANSFER) {
        body.loadUint(64); // queryId
        val amount = body.loadCoins();
        var st = lazy Storage.load();
        st.balance = st.balance + amount;
        st.save();
    }
}

get fun getWalletData(): (coins, address, address, cell) {
    val st = lazy Storage.load();
    return (st.balance, st.ownerAddr, st.masterAddr, st.walletCode);
}
