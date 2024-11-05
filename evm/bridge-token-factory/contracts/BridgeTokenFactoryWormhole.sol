// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {BridgeTokenFactory} from "./BridgeTokenFactory.sol";
import "./Borsh.sol";
import "./BridgeTypes.sol";

interface IWormhole {
    function publishMessage(
        uint32 nonce,
        bytes memory payload,
        uint8 consistencyLevel
    ) external payable returns (uint64 sequence);
}

enum MessageType {
    InitTransfer,
    FinTransfer,
    DeployToken
}

contract BridgeTokenFactoryWormhole is BridgeTokenFactory {
    IWormhole private _wormhole;
    // https://wormhole.com/docs/build/reference/consistency-levels
    uint8 private _consistencyLevel;
    uint32 public wormholeNonce;

    function initializeWormhole(
        address tokenImplementationAddress,
        address nearBridgeDerivedAddress,
        uint8 omniBridgeChainId,
        address wormholeAddress,
        uint8 consistencyLevel
    ) external initializer {
        initialize(tokenImplementationAddress, nearBridgeDerivedAddress, omniBridgeChainId);
        _wormhole = IWormhole(wormholeAddress);
        _consistencyLevel = consistencyLevel;
    }

    function deployTokenExtension(string memory token, address tokenAddress) internal override {
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            abi.encode(MessageType.DeployToken, token, tokenAddress),
            _consistencyLevel
        );

        wormholeNonce++;
    }

    function finTransferExtension(BridgeTypes.FinTransferPayload memory payload) internal override {
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            abi.encode(MessageType.FinTransfer, payload.token, payload.amount, payload.feeRecipient, payload.nonce),
            _consistencyLevel
        );

        wormholeNonce++;
    }

    function initTransferExtension(
        address sender,
        address tokenAddress,
        uint128 nonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message,
        uint256 value
    ) internal override {
        _wormhole.publishMessage{value: value}(
            wormholeNonce,
            abi.encode(
                MessageType.InitTransfer,
                sender,
                tokenAddress,
                nonce,
                amount,
                fee,
                nativeFee,
                recipient,
                message
            ),
            _consistencyLevel
        );

        wormholeNonce++;
    }
}