// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

import "../../common/Borsh.sol";
import {AccessControlUpgradeable} from '@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol';
import {UUPSUpgradeable} from '@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol';
import {IENear, INearProver} from './IENear.sol';
import {ICustomMinter} from '../../common/ICustomMinter.sol';
import "../../omni-bridge/contracts/SelectivePausableUpgradable.sol";

contract ENearProxy is UUPSUpgradeable, AccessControlUpgradeable, ICustomMinter, SelectivePausableUpgradable {
    IENear public eNear;

    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant PAUSE_ROLE = keccak256("PAUSE_ROLE");
    bytes public nearConnector;
    uint256 public currentReceiptId;
    INearProver public prover;

    uint constant PAUSED_LEGACY_FIN_TRANSFER = 1 << 0;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address _eNear, address _prover, bytes memory _nearConnector, uint256 _currentReceiptId, address _adminAddress) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();
        eNear = IENear(_eNear);
        nearConnector = _nearConnector;
        currentReceiptId = _currentReceiptId;
        prover = INearProver(_prover);
        _grantRole(DEFAULT_ADMIN_ROLE, _adminAddress);
        _grantRole(PAUSE_ROLE, _msgSender());
    }

    function mint(address token, address to, uint128 amount) public onlyRole(MINTER_ROLE) {
        require(token == address(eNear), "ERR_INCORRECT_ENEAR_ADDRESS");

        bytes memory fakeProofData = bytes.concat(
            new bytes(72),
            hex"01000000",
            abi.encodePacked(currentReceiptId),
            new bytes(24),
            abi.encodePacked(Borsh.swapBytes4(uint32(nearConnector.length))),
            abi.encodePacked(nearConnector),
            hex"022500000000",
            abi.encodePacked(Borsh.swapBytes16(amount)),
            abi.encodePacked(to),
            new bytes(280)
        );

        currentReceiptId += 1;
        eNear.finaliseNearToEthTransfer(fakeProofData, 0);
    }

    function burn(address token, uint128 amount) public onlyRole(MINTER_ROLE) {
        require(token == address(eNear), "ERR_INCORRECT_ENEAR_ADDRESS");
        eNear.transferToNear(amount, string(''));
    }

    function finaliseNearToEthTransfer(
        bytes memory proofData,
        uint64 proofBlockHeight
    ) external whenNotPaused(PAUSED_LEGACY_FIN_TRANSFER) {
        require(
            prover.proveOutcome(proofData, proofBlockHeight),
            "Proof should be valid"
        );

        eNear.finaliseNearToEthTransfer(proofData, proofBlockHeight);
    }

    function pauseAll() external onlyRole(PAUSE_ROLE) {
        _pause(PAUSED_LEGACY_FIN_TRANSFER);
    }

    function pause(uint flags) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _pause(flags);
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
