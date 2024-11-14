// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

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
            hex"000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            hex"01000000",
            abi.encodePacked(current_receipt_id),
            hex"000000000000000000000000000000000000000000000000",
            abi.encodePacked(swapBytes4(uint32(nearConnector.length))),
            abi.encodePacked(nearConnector),
            hex"022500000000",
            abi.encodePacked(swapBytes16(amount)),
            abi.encodePacked(to),
            hex"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        );

        current_receipt_id += 1;
        eNear.finaliseNearToEthTransfer(fakeProofData, 0);
    }

    function burn(address token, uint128 amount) public onlyRole(MINTER_ROLE) {
        require(token == address(eNear), "ERR_INCORRECT_ENEAR_ADDRESS");
        eNear.transferToNear(amount, string(''));
    }

    function swapBytes16(uint128 v) internal pure returns (uint128) {
        v = ((v & 0x00ff00ff00ff00ff00ff00ff00ff00ff) << 8) | ((v & 0xff00ff00ff00ff00ff00ff00ff00ff00) >> 8);
        v = ((v & 0x0000ffff0000ffff0000ffff0000ffff) << 16) | ((v & 0xffff0000ffff0000ffff0000ffff0000) >> 16);
        v = ((v & 0x00000000ffffffff00000000ffffffff) << 32) | ((v & 0xffffffff00000000ffffffff00000000) >> 32);
        return (v << 64) | (v >> 64);
    }

    function swapBytes4(uint32 v) internal pure returns (uint32) {
        v = ((v & 0x00ff00ff) << 8) | ((v & 0xff00ff00) >> 8);
        return (v << 16) | (v >> 16);
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
