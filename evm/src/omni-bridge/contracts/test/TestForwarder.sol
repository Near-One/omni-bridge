// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {Address} from "@openzeppelin/contracts/utils/Address.sol";

contract TestForwarder {
    function forward(
        address target,
        bytes calldata data
    ) external payable returns (bytes memory) {
        return Address.functionCallWithValue(target, data, msg.value);
    }
}
