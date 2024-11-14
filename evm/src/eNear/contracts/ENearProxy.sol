// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

import "rainbow-bridge-sol/nearbridge/contracts/Utils.sol";
import {AccessControlUpgradeable} from '@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol';
import {UUPSUpgradeable} from '@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol';
import {ENear} from './ENearABI.sol';
import {ICustomMinter} from '../../common/ICustomMinter.sol';

contract ENearProxy is UUPSUpgradeable, AccessControlUpgradeable, ICustomMinter {
    ENear public eNear;

    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes public nearConnector;
    uint256 public current_receipt_id;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address _eNear, bytes memory _nearConnector, uint256 _current_receipt_id) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();
        eNear = ENear(_eNear);
        nearConnector = _nearConnector;
        current_receipt_id = _current_receipt_id;
        _grantRole(DEFAULT_ADMIN_ROLE, _msgSender());
    }

    function mint(address token, address to, uint128 amount) public onlyRole(MINTER_ROLE) {
        require(token == address(eNear), "ERR_INCORRECT_ENEAR_ADDRESS");

        bytes memory fakeProofData = bytes.concat(
            new bytes(72),
            hex"01000000",
            abi.encodePacked(current_receipt_id),
            new bytes(24),
            abi.encodePacked(Utils.swapBytes4(uint32(nearConnector.length))),
            abi.encodePacked(nearConnector),
            hex"022500000000",
            abi.encodePacked(Utils.swapBytes16(amount)),
            abi.encodePacked(to),
            new bytes(280)
        );

        current_receipt_id += 1;
        eNear.finaliseNearToEthTransfer(fakeProofData, 0);
    }

    function burn(address token, uint128 amount) public onlyRole(MINTER_ROLE) {
        require(token == address(eNear), "ERR_INCORRECT_ENEAR_ADDRESS");
        eNear.transferToNear(amount, string(''));
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
