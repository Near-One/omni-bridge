// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {BridgeToken} from "./BridgeToken.sol";

/// @notice Hyperliquid-specific BridgeToken with two mint paths:
/// - 2-arg mint(address, uint256): mints on HyperEVM (tokens go directly to user)
/// - 3-arg mint(address, uint256, bytes): mints on HyperCore (includes _update to system address for spot-balance tracking)
contract HyperliquedBridgeToken is BridgeToken {
    address internal _systemAddress;

    function initialize(
        string memory name_,
        string memory symbol_,
        uint8 decimals_,
        address systemAddress_
    ) external initializer {
        __ERC20_init(name_, symbol_);
        __UUPSUpgradeable_init();
        __Ownable_init(_msgSender());

        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
        _systemAddress = systemAddress_;
    }

    function mint(
        address account,
        uint256 value,
        bytes memory
    ) external override onlyOwner {
        _mint(account, value);
        _update(account, _systemAddress, value);
    }
}
