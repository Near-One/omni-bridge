// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

contract TestWormhole {
    event MessagePublished(uint32 nonce, bytes payload, uint8 consistencyLevel);

    function publishMessage(
        uint32 nonce,
        bytes memory payload,
        uint8 consistencyLevel
    ) external payable returns (uint64) {
        emit MessagePublished(nonce, payload, consistencyLevel);
        return 0;
    }

    function messageFee() external pure returns (uint256) {
        return 10000;
    }
}
