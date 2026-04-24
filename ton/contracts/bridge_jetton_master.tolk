tolk 1.3

// Minimal bridge-minted jetton master. Locker-only mint; burn_notification
// from any wallet decrements totalSupply and forwards a BurnCompleteMsg to the
// locker (adminAddr) so it can emit InitTransferEvent for the NEAR-bound leg.
const OP_INTERNAL_TRANSFER = 0x178d4519
const OP_BURN_NOTIFICATION = 0x7bdd97de
const OP_BURN_COMPLETE = 0x6e910030

const ERR_NOT_FROM_ADMIN = 301
const ERR_UNKNOWN_OP = 0xFFFF

struct Storage {
    totalSupply: coins
    adminAddr: address // the OmniBridge locker
    walletCode: cell
    contentRef: cell // TEP-64 content cell (metadata); opaque to this contract
}

fun Storage.load() {
    return Storage.fromCell(contract.getData());
}

fun Storage.save(self) {
    contract.setData(self.toCell());
}

struct (0x642b7d07) MintMsg {
    queryId: uint64
    toAddr: address
    jettonAmount: coins
    forwardTonAmount: coins
}

// TEP-74 burn_notification from a jetton wallet. We trust the sender because
// the wallet is deployed deterministically from our stored walletCode — a
// spoofed master couldn't match a wallet that calls back to us.
struct (0x7bdd97de) BurnNotificationMsg {
    queryId: uint64
    amount: coins
    sender: address // original burn initiator (= owner of the burning JW)
    _responseDst: address
}

struct WalletStorageInit {
    balance: coins
    ownerAddr: address
    masterAddr: address
    walletCode: cell
}

fun onInternalMessage(in: InMessage) {
    // Empty body = deploy-only init message; stateInit already populated storage.
    if (in.body.isEmpty()) {
        return;
    }

    var slice = in.body;
    val op = slice.preloadUint(32);

    if (op == OP_BURN_NOTIFICATION) {
        handleBurnNotification(in.senderAddress, in.body);
        return;
    }

    assert (op == 0x642b7d07) throw ERR_UNKNOWN_OP;
    val msg = lazy MintMsg.fromSlice(in.body);
    var st = lazy Storage.load();

    assert (in.senderAddress == st.adminAddr) throw ERR_NOT_FROM_ADMIN;

    st.totalSupply = st.totalSupply + msg.jettonAmount;
    st.save();

    val walletStateInit = ContractState {
        code: st.walletCode,
        data: (WalletStorageInit {
            balance: 0 as coins,
            ownerAddr: msg.toAddr,
            masterAddr: contract.getAddress(),
            walletCode: st.walletCode,
        })
            .toCell(),
    };

    val transferBody = beginCell().storeUint(OP_INTERNAL_TRANSFER, 32).storeUint(msg.queryId, 64)
        .storeCoins(msg.jettonAmount)
        .storeAddress(contract.getAddress()) // from = master
        .storeAddress(in.senderAddress) // response = admin (locker)
        .storeCoins(msg.forwardTonAmount)
        .storeUint(0, 1) // forward_payload = empty (either-inline flag=0)
        .endCell();

    createMessage({
        bounce: true,
        dest: AutoDeployAddress { workchain: 0, stateInit: walletStateInit },
        value: 0 as coins,
        body: transferBody,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

fun onBouncedMessage(in: InMessageBounced) {
    // Two bounces we have to compensate for:
    //   - internal_transfer (mint) bouncing → JW deploy failed, roll back the
    //     totalSupply increment we did in handleMint.
    //   - burn_complete (to locker) bouncing → locker's handleBurnComplete
    //     threw, which shouldn't happen (it's been made bounce-proof), but if
    //     it does: re-increment totalSupply so supply accounting stays honest.
    //     The JW balance decrement can't be rolled back from here; operator
    //     intervention required.
    var body = in.bouncedBody;
    body.skipBouncedPrefix();
    if (body.remainingBitsCount() < 32 + 64) {
        return;
    }
    val op = body.loadUint(32);
    if (op == OP_INTERNAL_TRANSFER) {
        body.loadUint(64); // queryId
        val amount = body.loadCoins();
        var st = lazy Storage.load();
        st.totalSupply = st.totalSupply > amount ? st.totalSupply - amount : 0 as coins;
        st.save();
        return;
    }
    if (op == OP_BURN_COMPLETE && body.remainingBitsCount() >= 128) {
        body.loadUint(64); // queryId
        val amount = body.loadUint(128) as coins;
        var st = lazy Storage.load();
        st.totalSupply = st.totalSupply + amount;
        st.save();
    }
}

// TEP-74 burn_notification handler. Validates the sender matches the
// deterministic wallet address for `msg.sender`, then decrements totalSupply
// and forwards a BurnCompleteMsg to the locker (adminAddr) so it can emit
// InitTransferEvent for the matched burn queryId.
fun handleBurnNotification(from: address, bodySlice: slice) {
    val msg = lazy BurnNotificationMsg.fromSlice(bodySlice);
    var st = lazy Storage.load();

    // Anti-spoof: `from` must equal the deterministic JW address for msg.sender.
    // A fake wallet can't impersonate another (master, owner) wallet because
    // its contract-address-hash wouldn't match.
    val expectedJw = (AutoDeployAddress {
        workchain: 0,
        stateInit: ContractState {
            code: st.walletCode,
            data: (WalletStorageInit {
                balance: 0 as coins,
                ownerAddr: msg.sender,
                masterAddr: contract.getAddress(),
                walletCode: st.walletCode,
            })
                .toCell(),
        },
    })
        .calculateAddress();
    assert (from == expectedJw) throw ERR_NOT_FROM_ADMIN;

    st.totalSupply = st.totalSupply > msg.amount ? st.totalSupply - msg.amount : 0 as coins;
    st.save();

    // Tell the locker the burn landed so it can emit InitTransferEvent.
    val notifBody = beginCell().storeUint(OP_BURN_COMPLETE, 32).storeUint(msg.queryId, 64).storeUint(
        msg.amount as uint128,
        128
    )
        .endCell();
    createMessage({
        bounce: true,
        dest: st.adminAddr,
        value: 0 as coins,
        body: notifBody,
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

get fun getJettonData(): (coins, address, cell, cell) {
    val st = lazy Storage.load();
    return (st.totalSupply, st.adminAddr, st.contentRef, st.walletCode);
}

get fun getWalletAddress(ownerAddr: address): address {
    val st = lazy Storage.load();
    return (AutoDeployAddress {
        workchain: 0,
        stateInit: ContractState {
            code: st.walletCode,
            data: (WalletStorageInit {
                balance: 0 as coins,
                ownerAddr,
                masterAddr: contract.getAddress(),
                walletCode: st.walletCode,
            })
                .toCell(),
        },
    })
        .calculateAddress();
}
