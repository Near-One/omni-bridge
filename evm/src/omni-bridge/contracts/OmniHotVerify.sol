// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {OmniBridge} from "./OmniBridge.sol";

contract OmniHotVerify is OmniBridge {
    function initiatedTransfers(uint64 nonce) public view returns (bytes32) {
        return _initiatedTransfers[nonce];
    }

    function recordInitiatedTransfer(
        address sender,
        address tokenAddress,
        uint64 originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message
    ) internal virtual override {
        bytes32 transferHash = _hashInitiatedTransfer(
            sender,
            tokenAddress,
            originNonce,
            amount,
            fee,
            nativeFee,
            recipient,
            message
        );
        require(
            _initiatedTransfers[originNonce] == bytes32(0),
            "ERR_NONCE_ALREADY_USED"
        );
        _initiatedTransfers[originNonce] = transferHash;
    }

    function hotVerify(
        bytes32 msg_hash,
        bytes memory /*_walletId*/,
        bytes memory userPayload,
        bytes memory /*_metadata*/
    ) public view returns (bool) {
        uint64 nonce = abi.decode(userPayload, (uint64));
        if (nonce == 0) return false;
        bytes32 stored = _initiatedTransfers[nonce];
        if (stored == bytes32(0)) return false;
        return stored == msg_hash;
    }

    function _hashInitiatedTransfer(
        address sender,
        address tokenAddress,
        uint64 originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message
    ) internal view returns (bytes32) {
        return
            keccak256(
                abi.encode(
                    block.chainid,
                    address(this),
                    sender,
                    tokenAddress,
                    originNonce,
                    amount,
                    fee,
                    nativeFee,
                    recipient,
                    message
                )
            );
    }
}
