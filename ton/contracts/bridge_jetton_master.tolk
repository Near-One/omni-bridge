tolk 1.3

// Minimal bridge-minted jetton master: locker-only mint, no TEP-89 / burn_notification yet.
const OP_INTERNAL_TRANSFER = 0x178d4519

const ERR_NOT_FROM_ADMIN = 301

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
        body: UnsafeBodyNoRef { forceInline: transferBody.beginParse() },
    })
        .send(SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE);
}

fun onBouncedMessage(in: InMessageBounced) {
    // Roll back the totalSupply increment from the failed mint.
    var body = in.bouncedBody;
    body.skipBouncedPrefix();
    if (body.remainingBitsCount() < 32 + 64) {
        return;
    }
    val op = body.loadUint(32);
    if (op != OP_INTERNAL_TRANSFER) {
        return;
    }
    body.loadUint(64); // queryId
    val amount = body.loadCoins();

    var st = lazy Storage.load();
    st.totalSupply = st.totalSupply > amount ? st.totalSupply - amount : 0 as coins;
    st.save();
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
