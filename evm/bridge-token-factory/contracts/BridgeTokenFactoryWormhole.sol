// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {BridgeTokenFactory} from "./BridgeTokenFactory.sol";
import "./Borsh.sol";

interface IWormhole {
    function publishMessage(
        uint32 nonce,
        bytes memory payload,
        uint8 consistencyLevel
    ) external payable returns (uint64 sequence);
}

contract BridgeTokenFactoryWormhole is BridgeTokenFactory {
    IWormhole private _wormhole;
    // https://wormhole.com/docs/build/reference/consistency-levels
    uint8 private _consistencyLevel;
    uint32 public wormholeNonce;

    function initializeWormhole(
        address _tokenImplementationAddress,
        address _nearBridgeDerivedAddress,
        address wormholeAddress,
        uint8 consistencyLevel
    ) external initializer {
        initialize(_tokenImplementationAddress, _nearBridgeDerivedAddress);
        _wormhole = IWormhole(wormholeAddress);
        _consistencyLevel = consistencyLevel;
    }

    function deployTokenExtension(string memory token, address tokenAddress) internal override {
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            abi.encode(token, tokenAddress),
            _consistencyLevel
        );

        wormholeNonce++;
    }

    function depositExtension(BridgeDeposit memory bridgeDeposit) internal override {
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            abi.encode(bridgeDeposit.token, bridgeDeposit.amount, bridgeDeposit.feeRecipient, bridgeDeposit.nonce),
            _consistencyLevel
        );

        wormholeNonce++;

    }

    function withdrawExtension(string memory token, uint128 amount, string memory recipient, address sender) internal override {
        _wormhole.publishMessage{value: msg.value}(
            wormholeNonce,
            abi.encode(token, amount, recipient, sender),
            _consistencyLevel
        );

        wormholeNonce++;
    }
}