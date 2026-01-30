// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {ERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import {Ownable2StepUpgradeable} from "@openzeppelin/contracts-upgradeable/access/Ownable2StepUpgradeable.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {BridgeToken} from "./BridgeToken.sol";

contract HyperliquedBridgeToken is BridgeToken {
    address internal _systemAddress;

    function initialize(
        // todo:: verify if it is overridden correctly
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

    function mintWithMsg(
        address beneficiary,
        uint256 amount,
        bytes memory
    ) external onlyOwner {
        _mint(beneficiary, amount);
        _update(beneficiary, _systemAddress, amount);
    }
}
