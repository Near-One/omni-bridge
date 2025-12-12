// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

interface ICustomMinter {
    function mint(address token, address to, uint128 amount) external;
    function burn(address token, uint128 amount) external;
}
