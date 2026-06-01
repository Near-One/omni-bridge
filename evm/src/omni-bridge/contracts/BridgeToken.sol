// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {ERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import {Ownable2StepUpgradeable} from "@openzeppelin/contracts-upgradeable/access/Ownable2StepUpgradeable.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {IBridgeToken} from "../../common/IBridgeToken.sol";

contract BridgeToken is
    Initializable,
    UUPSUpgradeable,
    ERC20Upgradeable,
    Ownable2StepUpgradeable,
    IBridgeToken
{
    string internal _name;
    string internal _symbol;
    uint8 internal _decimals;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        string memory name_,
        string memory symbol_,
        uint8 decimals_
    ) external initializer {
        __ERC20_init(name_, symbol_);
        __UUPSUpgradeable_init();
        __Ownable_init(_msgSender());

        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
    }

    function setMetadata(
        string memory name_,
        string memory symbol_,
        uint8 decimals_
    ) external onlyOwner {
        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
    }

    function mint(address beneficiary, uint256 amount) external onlyOwner {
        _mint(beneficiary, amount);
    }

    function mint(
        address account,
        uint256 value,
        bytes memory
    ) external virtual onlyOwner {
        _mint(account, value);
    }

    function burn(address account, uint256 value) external onlyOwner {
        _burn(account, value);
    }

    function name() public view virtual override returns (string memory) {
        return _name;
    }

    function symbol() public view virtual override returns (string memory) {
        return _symbol;
    }

    function decimals() public view virtual override returns (uint8) {
        return _decimals;
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyOwner {}
}
