tolk 1.3

// Minimal TEP-74 jetton wallet: inbound mint via internal_transfer from the master only.
const OP_INTERNAL_TRANSFER = 0x178d4519

const ERR_NOT_FROM_MASTER = 401

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

struct (0x178d4519) InternalTransferMsg {
    queryId: uint64
    amount: coins
    fromAddr: address
    responseAddr: address
    forwardTonAmount: coins
    forwardPayload: cell
}

fun onInternalMessage(in: InMessage) {
    val op = in.body.remainingBitsCount() >= 32 ? in.body.preloadUint(32) : 0;

    if (op == OP_INTERNAL_TRANSFER) {
        var st = lazy Storage.load();
        assert (in.senderAddress == st.masterAddr) throw ERR_NOT_FROM_MASTER;

        val msg = lazy InternalTransferMsg.fromSlice(in.body);
        st.balance = st.balance + msg.amount;
        st.save();
        return;
    }
}
// Anything else (plain TON top-up, empty msg) is a no-op.
fun onBouncedMessage(in: InMessageBounced) {}

get fun getWalletData(): (coins, address, address, cell) {
    val st = lazy Storage.load();
    return (st.balance, st.ownerAddr, st.masterAddr, st.walletCode);
}
