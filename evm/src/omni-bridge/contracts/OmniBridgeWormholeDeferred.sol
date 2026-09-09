// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {OmniBridgeWormhole} from "./OmniBridgeWormhole.sol";

/// @notice OmniBridgeWormhole variant that *queues* the InitTransfer Wormhole
/// message instead of publishing it inline, so the `LogMessagePublished` log is
/// emitted from an ordinary signed transaction.
///
/// # Why
///
/// On HyperEVM, `HyperliquedBridgeToken.coreReceiveWithData` is invoked by the
/// HyperCore system address, which means `initTransfer` — and therefore
/// `IWormhole.publishMessage` — executes inside a HyperCore *system
/// transaction*. Those transactions are not indexed into HyperEVM's log index:
/// the resulting log is absent from `eth_getLogs` and the transaction is absent
/// from the block's transaction list, though both are visible via
/// `eth_getTransactionReceipt`.
///
/// Wormhole guardians only observe messages through `WatchLogMessagePublished`
/// (`eth_subscribe("logs")` or `eth_getLogs` polling), so a message published
/// from a system transaction is never observed and no VAA is ever produced.
/// Waiting does not help — the log is permanently missing from the index, not
/// late.
///
/// # How
///
/// `initTransferExtension` commits `keccak256(payload)` to storage and returns.
/// A permissionless keeper later calls `publishQueuedTransfer` with the full
/// payload from an ordinary EOA transaction; the contract verifies the hash and
/// publishes. The burn and the `InitTransfer` event still happen atomically in
/// the queueing transaction, so the "event-transfer atomicity" invariant holds:
/// a queued entry only ever exists for tokens that are already burned/locked.
///
/// # Storage strategy: hash, not payload
///
/// Only the 32-byte hash is stored. Storing the whole payload would cost one
/// cold `SSTORE` per 32-byte word (a 161-byte payload is 6 data words plus a
/// length word), and HyperCore system transactions run under a hard ~200k gas
/// budget that a 7-slot write would blow. The keeper reconstructs the payload
/// off-chain and the hash makes the reconstruction non-forgeable.
///
/// # Keeper contract
///
/// Discovery cannot use logs (a log emitted from a system transaction is
/// invisible for exactly the same reason the Wormhole log was). Instead:
///   1. read `currentOriginNonce` for the upper bound of the scan range,
///   2. for each candidate nonce, read `queuedPayloadHash(nonce)` — non-zero
///      means outstanding work,
///   3. rebuild the payload from the HyperCore action (`from`, `amount`, and the
///      `fee`/`recipient`/`message` in its `data`) or from the `InitTransfer`
///      event in the queueing transaction's receipt, verify locally against
///      `queuedPayloadHash`, then submit.
/// `encodeQueuedTransferPayload` is exposed so a keeper can build candidate
/// payloads with the exact on-chain encoder rather than reimplementing Borsh.
///
/// # Upgrade safety
///
/// `queuedPayloadHash` is appended after `OmniBridgeWormhole`'s slot
/// (`_wormhole` + `_consistencyLevel` + `wormholeNonce` pack into one slot), so
/// an existing OmniBridgeWormhole proxy can be upgraded to this contract in
/// place. `OmniBridgeWormhole` has no trailing `__gap`, so any new variable
/// added there in the future would collide with the variables below — append
/// new `OmniBridgeWormhole` state only with a matching change here.
// slither-disable-start unused-return
contract OmniBridgeWormholeDeferred is OmniBridgeWormhole {
    /// @notice keccak256 of the queued InitTransfer Wormhole payload, keyed by the
    /// `originNonce` that `OmniBridge.initTransfer` assigned to the transfer.
    /// Zero means "nothing queued" (either never queued, or already published).
    ///
    /// @dev There is deliberately no "highest queued nonce" counter: every
    /// `initTransfer` on this contract queues, so `currentOriginNonce` already
    /// bounds the keeper's scan range and a second counter would cost another
    /// ~5k gas per transfer inside the constrained system transaction.
    mapping(uint64 => bytes32) public queuedPayloadHash;

    event InitTransferQueued(
        uint64 indexed originNonce,
        bytes32 payloadHash,
        uint256 payloadLength
    );

    event QueuedTransferPublished(
        uint64 indexed originNonce,
        uint32 wormholeNonce,
        uint64 sequence,
        address publisher
    );

    /// @dev No entry queued under this `originNonce` — never queued, or already published.
    error NothingQueued(uint64 originNonce);
    /// @dev `keccak256(payload)` does not match the committed hash.
    error PayloadMismatch(uint64 originNonce);
    /// @dev The Wormhole message fee is paid by the publisher, not the initiator.
    /// Queueing must not carry value, or it would be stranded in the contract.
    error UnexpectedValue(uint256 value);

    /// @dev Commits the payload hash instead of publishing. Deliberately performs
    /// no external call, so it stays within the HyperCore system-transaction gas
    /// budget and cannot reenter.
    function initTransferExtension(
        address sender,
        address tokenAddress,
        uint64 originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message,
        uint256 value
    ) internal override {
        // The message fee is paid at publish time by whoever calls
        // `publishQueuedTransfer`. Any value forwarded here would be
        // unattributable, so refuse it rather than silently keep it.
        if (value != 0) {
            revert UnexpectedValue(value);
        }

        bytes memory payload = encodeInitTransferPayload(
            sender,
            tokenAddress,
            originNonce,
            amount,
            fee,
            nativeFee,
            recipient,
            message
        );

        bytes32 payloadHash = keccak256(payload);
        queuedPayloadHash[originNonce] = payloadHash;

        emit InitTransferQueued(originNonce, payloadHash, payload.length);
    }

    /// @notice Publishes a previously queued InitTransfer message to Wormhole.
    /// Permissionless: the committed hash is the only authority needed, so any
    /// keeper can flush the queue and a stuck message is never operator-gated.
    /// @param originNonce The transfer's origin nonce, as assigned by `initTransfer`.
    /// @param payload The exact payload committed at queue time.
    /// @return sequence The Wormhole sequence number assigned to the message.
    function publishQueuedTransfer(
        uint64 originNonce,
        bytes calldata payload
    ) external payable returns (uint64 sequence) {
        bytes32 committed = queuedPayloadHash[originNonce];
        if (committed == bytes32(0)) {
            revert NothingQueued(originNonce);
        }
        if (keccak256(payload) != committed) {
            revert PayloadMismatch(originNonce);
        }

        // SECURITY: clear the entry and bump the nonce before the external call,
        // so a reentrant call cannot publish the same transfer twice.
        delete queuedPayloadHash[originNonce];
        uint32 nonce = wormholeNonce;
        wormholeNonce = nonce + 1;

        // slither-disable-next-line reentrancy-eth,reentrancy-events
        sequence = _wormhole.publishMessage{value: msg.value}(
            nonce,
            payload,
            _consistencyLevel
        );

        emit QueuedTransferPublished(
            originNonce,
            nonce,
            sequence,
            _msgSender()
        );
    }

    /// @notice Builds the InitTransfer payload with the same encoder the queueing
    /// path uses, so a keeper can reproduce the committed bytes exactly.
    /// @dev View-only helper; `sender` and `tokenAddress` are both the bridge
    /// token address on the HyperCore path.
    function encodeQueuedTransferPayload(
        address sender,
        address tokenAddress,
        uint64 originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message
    ) external view returns (bytes memory) {
        return
            encodeInitTransferPayload(
                sender,
                tokenAddress,
                originNonce,
                amount,
                fee,
                nativeFee,
                recipient,
                message
            );
    }

    /// @notice Whether `originNonce` still has an unpublished message queued.
    function isQueued(uint64 originNonce) external view returns (bool) {
        return queuedPayloadHash[originNonce] != bytes32(0);
    }
}
// slither-disable-end unused-return
