// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

interface INearProver {
    function proveOutcome(bytes calldata proofData, uint64 blockHeight) external view returns (bool);
}

contract FakeProver is INearProver {
    function proveOutcome(bytes calldata, uint64) external pure returns (bool) {
        return true;
    }
}
