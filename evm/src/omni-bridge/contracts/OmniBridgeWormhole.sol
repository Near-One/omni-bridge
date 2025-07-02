// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {OmniBridge} from "./OmniBridge.sol";
import "../../common/Borsh.sol";
import "./BridgeTypes.sol";

interface IWormhole {
    function publishMessage(
        uint32 nonce,
        bytes memory payload,
        uint8 consistencyLevel
    ) external payable returns (uint64 sequence);

    function messageFee() external view returns (uint256);
}

enum MessageType {
    InitTransfer,
    FinTransfer,
    DeployToken,
    LogMetadata
}

// slither-disable-start unused-return
contract OmniBridgeWormhole is OmniBridge {
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

    function deployTokenExtension(string memory token, address tokenAddress, uint8 decimals, uint8 originDecimals) internal override {
        bytes memory payload = bytes.concat(
            bytes1(uint8(MessageType.DeployToken)),
            Borsh.encodeString(token),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(tokenAddress),
            bytes1(decimals),
            bytes1(originDecimals)
        );
        // slither-disable-next-line reentrancy-eth
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            payload,
            _consistencyLevel
        );

        wormholeNonce++;
    }


    function logMetadataExtension(
        address tokenAddress,
        string memory name,
        string memory symbol,
        uint8 decimals
    ) internal override {
        bytes memory payload = bytes.concat(
            bytes1(uint8(MessageType.LogMetadata)),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(tokenAddress),
            Borsh.encodeString(name),
            Borsh.encodeString(symbol),
            bytes1(decimals)
        );
        // slither-disable-next-line reentrancy-eth
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            payload,
            _consistencyLevel
        );

        wormholeNonce++;
    }

    function finTransferExtension(BridgeTypes.TransferMessagePayload memory payload) internal override {
        bytes memory messagePayload = bytes.concat(
            bytes1(uint8(MessageType.FinTransfer)),
            bytes1(payload.originChain),
            Borsh.encodeUint64(payload.originNonce),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(payload.tokenAddress),
            Borsh.encodeUint128(payload.amount),
            Borsh.encodeString(payload.feeRecipient)
        );
        // slither-disable-next-line reentrancy-eth
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            messagePayload,
            _consistencyLevel
        );

        wormholeNonce++;
    }

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
        bytes memory payload = bytes.concat(
            bytes1(uint8(MessageType.InitTransfer)),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(sender),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(tokenAddress),
            Borsh.encodeUint64(originNonce),
            Borsh.encodeUint128(amount),
            Borsh.encodeUint128(fee),
            Borsh.encodeUint128(nativeFee),
            Borsh.encodeString(recipient),
            Borsh.encodeString(message)
        );
        // slither-disable-next-line reentrancy-eth
        _wormhole.publishMessage{value: value}(
            wormholeNonce,
            payload,
            _consistencyLevel
        );

        wormholeNonce++;
    }

    function setWormholeAddress(
        address wormholeAddress,
        uint8 consistencyLevel
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _wormhole = IWormhole(wormholeAddress);
        _consistencyLevel = consistencyLevel;
    }
}
// slither-disable-end unused-return
