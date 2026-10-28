// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {AccessControlEnumerableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/extensions/AccessControlEnumerableUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {EnumerableSet} from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";
import {ITrustedRelayerRegistry} from "../../common/ITrustedRelayerRegistry.sol";

contract TrustedRelayerRegistry is
    UUPSUpgradeable,
    AccessControlEnumerableUpgradeable,
    ITrustedRelayerRegistry
{
    using EnumerableSet for EnumerableSet.AddressSet;

    struct RelayerState {
        uint128 stake;
        uint64 activateAt;
    }

    struct RelayerConfig {
        uint128 stakeRequired;
        uint64 waitingPeriod;
    }

    struct RelayerEntry {
        address relayer;
        uint128 stake;
        uint64 activateAt;
    }

    bytes32 public constant TRUSTED_RELAYER_ROLE =
        keccak256("TRUSTED_RELAYER_ROLE");
    bytes32 public constant RELAYER_MANAGER_ROLE =
        keccak256("RELAYER_MANAGER_ROLE");

    RelayerConfig public relayerConfig;
    mapping(address => RelayerState) public relayers;
    EnumerableSet.AddressSet private stakedRelayers;

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

    function initialize(
        address admin,
        uint128 stakeRequired,
        uint64 waitingPeriod
    ) public initializer {
        __UUPSUpgradeable_init();
        __AccessControlEnumerable_init();
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(RELAYER_MANAGER_ROLE, admin);
        _setRelayerConfig(stakeRequired, waitingPeriod);
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
        stakedRelayers.add(msg.sender);

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
        stakedRelayers.remove(msg.sender);

        emit RelayerResigned(msg.sender, state.stake);

        _sendStake(msg.sender, state.stake);
    }

    function rejectRelayerApplication(
        address relayer
    ) external onlyRole(RELAYER_MANAGER_ROLE) {
        RelayerState memory state = relayers[relayer];

        if (state.stake == 0) {
            revert RelayerNotFound();
        }

        delete relayers[relayer];
        stakedRelayers.remove(relayer);

        emit RelayerRejected(relayer, state.stake, msg.sender);

        _sendStake(msg.sender, state.stake);
    }

    function setRelayerConfig(
        uint128 stakeRequired,
        uint64 waitingPeriod
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _setRelayerConfig(stakeRequired, waitingPeriod);
    }

    function getActiveRelayers(
        uint256 fromIndex,
        uint256 limit
    ) external view returns (RelayerEntry[] memory) {
        return _getStakedRelayers(true, fromIndex, limit);
    }

    function getPendingRelayers(
        uint256 fromIndex,
        uint256 limit
    ) external view returns (RelayerEntry[] memory) {
        return _getStakedRelayers(false, fromIndex, limit);
    }

    function _getStakedRelayers(
        bool active,
        uint256 fromIndex,
        uint256 limit
    ) private view returns (RelayerEntry[] memory) {
        uint256 length = stakedRelayers.length();
        address[] memory matching = new address[](length);
        uint256 matchingCount;
        for (uint256 i; i < length; ++i) {
            address relayer = stakedRelayers.at(i);
            if ((block.timestamp >= relayers[relayer].activateAt) == active) {
                matching[matchingCount++] = relayer;
            }
        }

        if (fromIndex >= matchingCount) {
            return new RelayerEntry[](0);
        }
        uint256 size = matchingCount - fromIndex;
        if (size > limit) {
            size = limit;
        }

        RelayerEntry[] memory entries = new RelayerEntry[](size);
        for (uint256 i; i < size; ++i) {
            address relayer = matching[fromIndex + i];
            RelayerState memory state = relayers[relayer];
            entries[i] = RelayerEntry({
                relayer: relayer,
                stake: state.stake,
                activateAt: state.activateAt
            });
        }

        return entries;
    }

    function _setRelayerConfig(
        uint128 stakeRequired,
        uint64 waitingPeriod
    ) private {
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

    uint256[50] private __gap;
}
