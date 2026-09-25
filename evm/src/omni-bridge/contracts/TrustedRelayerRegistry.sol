// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {AccessControlUpgradeable} from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {ITrustedRelayerRegistry} from "../../common/ITrustedRelayerRegistry.sol";

contract TrustedRelayerRegistry is
    UUPSUpgradeable,
    AccessControlUpgradeable,
    ITrustedRelayerRegistry
{
    struct RelayerState {
        uint128 stake;
        uint64 activateAt;
    }

    struct RelayerConfig {
        uint128 stakeRequired;
        uint64 waitingPeriod;
    }

    bytes32 public constant TRUSTED_RELAYER_ROLE =
        keccak256("TRUSTED_RELAYER_ROLE");
    bytes32 public constant RELAYER_MANAGER_ROLE =
        keccak256("RELAYER_MANAGER_ROLE");

    RelayerConfig public relayerConfig;
    mapping(address => RelayerState) public relayers;

    event RelayerApplied(
        address indexed relayer,
        uint128 stake,
        uint64 activateAt
    );
    event RelayerResigned(address indexed relayer, uint128 stake);
    event RelayerRejected(
        address indexed relayer,
        uint128 stake,
        address indexed manager
    );
    event RelayerConfigSet(uint128 stakeRequired, uint64 waitingPeriod);

    error RelayerStakingDisabled();
    error RelayerApplicationExists();
    error InvalidRelayerStake(uint256 provided, uint256 required);
    error RelayerNotFound();
    error RelayerNotActive();
    error FailedToSendStake();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address admin) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
    }

    function isTrustedRelayer(address account) external view returns (bool) {
        if (hasRole(TRUSTED_RELAYER_ROLE, account)) {
            return true;
        }

        RelayerState memory state = relayers[account];
        return state.stake != 0 && block.timestamp >= state.activateAt;
    }

    function applyForTrustedRelayer() external payable {
        RelayerConfig memory config = relayerConfig;

        if (config.stakeRequired == 0) {
            revert RelayerStakingDisabled();
        }
        if (relayers[msg.sender].stake != 0) {
            revert RelayerApplicationExists();
        }
        if (msg.value != config.stakeRequired) {
            revert InvalidRelayerStake(msg.value, config.stakeRequired);
        }

        uint64 activateAt = uint64(block.timestamp) + config.waitingPeriod;
        relayers[msg.sender] = RelayerState({
            stake: config.stakeRequired,
            activateAt: activateAt
        });

        emit RelayerApplied(msg.sender, config.stakeRequired, activateAt);
    }

    function resignTrustedRelayer() external {
        RelayerState memory state = relayers[msg.sender];

        if (state.stake == 0) {
            revert RelayerNotFound();
        }
        if (block.timestamp < state.activateAt) {
            revert RelayerNotActive();
        }

        delete relayers[msg.sender];

        emit RelayerResigned(msg.sender, state.stake);

        _sendStake(msg.sender, state.stake);
    }

    function rejectRelayerApplication(address relayer) external {
        if (
            !hasRole(RELAYER_MANAGER_ROLE, msg.sender) &&
            !hasRole(DEFAULT_ADMIN_ROLE, msg.sender)
        ) {
            revert AccessControlUnauthorizedAccount(
                msg.sender,
                RELAYER_MANAGER_ROLE
            );
        }

        RelayerState memory state = relayers[relayer];

        if (state.stake == 0) {
            revert RelayerNotFound();
        }

        delete relayers[relayer];

        emit RelayerRejected(relayer, state.stake, msg.sender);

        _sendStake(msg.sender, state.stake);
    }

    function setRelayerConfig(
        uint128 stakeRequired,
        uint64 waitingPeriod
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        relayerConfig = RelayerConfig({
            stakeRequired: stakeRequired,
            waitingPeriod: waitingPeriod
        });

        emit RelayerConfigSet(stakeRequired, waitingPeriod);
    }

    function _sendStake(address to, uint128 amount) private {
        (bool success, ) = to.call{value: amount}("");
        if (!success) revert FailedToSendStake();
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}

    uint256[48] private __gap;
}
