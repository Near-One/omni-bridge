// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

interface IBridgeToken {
    function mint(address beneficiary, uint256 amount) external;
    function mintWithMsg(address beneficiary, uint256 amount, bytes memory message) external;
    function burn(address act, uint256 amount) external;
}
