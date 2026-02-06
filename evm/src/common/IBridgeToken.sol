// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

interface IBridgeToken {
    function mint(address account, uint256 value) external;

    function mint(
        address account,
        uint256 value,
        bytes memory message
    ) external;

    function burn(address account, uint256 value) external;
}
