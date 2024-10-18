// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.27;

interface ENear {
    function transferToNear(uint256 _amount, string calldata _nearReceiverAccountId) external;
    function finaliseNearToEthTransfer(bytes calldata proofData, uint64 proofBlockHeight) external;
    function nominateAdmin(address newAdmin) external;
    function acceptAdmin(address newAdmin) external;
    function adminSstore(uint key, uint value) external;
}