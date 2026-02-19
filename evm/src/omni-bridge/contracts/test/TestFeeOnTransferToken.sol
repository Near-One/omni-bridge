// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract TestFeeOnTransferToken is ERC20 {
    uint256 public immutable feeBps;

    constructor(
        string memory name_,
        string memory symbol_,
        address initialHolder,
        uint256 initialSupply,
        uint256 feeBps_
    ) ERC20(name_, symbol_) {
        require(feeBps_ <= 10_000, "invalid fee bps");
        feeBps = feeBps_;
        _mint(initialHolder, initialSupply);
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function _update(address from, address to, uint256 value) internal override {
        // Do not charge transfer fee on mint/burn.
        if (from == address(0) || to == address(0) || feeBps == 0) {
            super._update(from, to, value);
            return;
        }

        uint256 fee = (value * feeBps) / 10_000;
        uint256 net = value - fee;

        if (fee > 0) {
            super._update(from, address(0), fee);
        }
        super._update(from, to, net);
    }
}
