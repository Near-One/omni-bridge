// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {OmniBridgeWormhole} from "./OmniBridgeWormhole.sol";

// slither-disable-start unused-return
contract OmniBridgeWormholeDeferred is OmniBridgeWormhole {
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

    error NothingQueued(uint64 originNonce);
    error PayloadMismatch(uint64 originNonce);
    error UnexpectedValue(uint256 value);

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

    /// @notice Publishes a queued InitTransfer message to Wormhole. Permissionless
    /// — the commitment is the only authority needed, so a stuck message is never
    /// operator-gated.
    /// @param originNonce The transfer's origin nonce, as assigned by `initTransfer`.
    /// @param payload The exact payload committed at queue time.
    /// @return sequence The Wormhole sequence assigned to the message.
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

        delete queuedPayloadHash[originNonce];
        uint32 nonce = wormholeNonce++;

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
}
// slither-disable-end unused-return
