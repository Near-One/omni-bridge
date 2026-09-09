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
        // The message fee is paid by whoever publishes. Value forwarded here
        // would be unattributable, so refuse it rather than strand it.
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

    /// @notice Builds a payload with the same encoder the queueing path uses, so a
    /// keeper can reproduce the committed bytes without reimplementing Borsh.
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
