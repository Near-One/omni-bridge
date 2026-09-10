// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {BridgeToken} from "./BridgeToken.sol";

interface IOmniBridgeQueueInitTransfer {
    function queueInitTransfer(
        address sender,
        uint64 coreNonce,
        uint128 amount,
        uint128 fee,
        string calldata recipient,
        string calldata message
    ) external returns (uint64 originNonce);
}

interface ICoreReceiveWithData {
    function coreReceiveWithData(
        address from,
        bytes32 destinationRecipient,
        uint32 destinationChainId,
        uint256 amount,
        uint64 coreNonce,
        bytes calldata data
    ) external;
}

/// @notice Hyperliquid-specific BridgeToken with two mint paths:
/// - 2-arg mint(address, uint256): mints on HyperEVM (tokens go directly to user)
/// - 3-arg mint(address, uint256, bytes): mints on HyperCore (includes _update to system address for spot-balance tracking)
contract HyperliquedBridgeToken is BridgeToken, ICoreReceiveWithData {
    using SafeCast for uint256;

    address internal _systemAddress;
    bytes32 constant HYPER_CORE_DEPLOYER_SLOT = keccak256("HyperCore deployer");
    event HyperCoreDeployerSet(address indexed deployer);

    uint8 public constant ACTION_TRANSFER = 0;
    uint8 public constant ACTION_INIT_TRANSFER = 1;

    event CoreReceived(
        address indexed sender,
        uint8 indexed action,
        uint64 indexed coreNonce,
        uint256 amount,
        bytes data
    );

    error NotSystemAddress();
    error EmptyActionData();
    error UnknownAction(uint8 action);
    error InvalidRecipient();

    function initialize(
        string memory name_,
        string memory symbol_,
        uint8 decimals_,
        address systemAddress_,
        address hyperCoreDeployer_
    ) external initializer {
        __ERC20_init(name_, symbol_);
        __UUPSUpgradeable_init();
        __Ownable_init(_msgSender());

        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
        _systemAddress = systemAddress_;

        bytes32 hyperCoreDeployerSlot = HYPER_CORE_DEPLOYER_SLOT;
        assembly {
            sstore(hyperCoreDeployerSlot, hyperCoreDeployer_)
        }
        emit HyperCoreDeployerSet(hyperCoreDeployer_);
    }

    function mint(
        address account,
        uint256 value,
        bytes memory
    ) external override onlyOwner {
        _mint(account, value);
        _update(account, _systemAddress, value);
    }

    /// @notice HyperCore -> HyperEVM callback invoked by the system address when a
    /// HyperCore user triggers `sendToEvmWithData` targeting this token.
    /// `destinationRecipient` and `destinationChainId` are unused; all routing info
    /// comes from `data`. `coreNonce` is the HyperCore-side sequence number, kept
    /// for correlation with the HL action; it is sequenced per sender and so is not
    /// used as a key.
    /// @dev Accounting model: the 3-arg `mint` parks HyperCore-bound tokens at
    /// `_systemAddress`, so that account holds the standing pool that mirrors total
    /// HyperCore-side balance. HyperLiquid does NOT pre-transfer tokens before this
    /// call fires, so we pull from `_systemAddress` ourselves; an insufficient pool
    /// is a safe revert that signals accounting drift between HyperCore and HyperEVM.
    ///
    /// Dispatch:
    /// - 0x00 || abi.encode(address recipient): release `amount` from the pool to
    ///   the HyperEVM `recipient`.
    /// - 0x01 || abi.encode(uint128 fee, string recipient, string message): move
    ///   `amount` from the pool to this contract and commit it under `coreNonce`.
    ///   `initTransfer` is NOT called inline: this call's logs are absent from the
    ///   block's `logsBloom` (HyperCore system tx) and thus invisible to filtered
    ///   `eth_getLogs`/`eth_subscribe` watchers (Wormhole guardians, our indexer).
    ///   `triggerPendingInitTransfer` submits it later from a normal tx.
    ///   `recipient` is an OmniAddress string (e.g. `near:alice.near`); nativeFee = 0.
    ///   The resulting InitTransfer event carries `sender = address(this)`, so the
    ///   NEAR side cannot recover the originating HyperCore user from this path.
    function coreReceiveWithData(
        address from,
        bytes32 /*destinationRecipient*/,
        uint32 /*destinationChainId*/,
        uint256 amount,
        uint64 coreNonce,
        bytes calldata data
    ) external override {
        if (msg.sender != _systemAddress) revert NotSystemAddress();
        if (data.length == 0) revert EmptyActionData();

        uint8 action = uint8(data[0]);
        bytes calldata tail = data[1:];

        if (action == ACTION_TRANSFER) {
            address recipient = abi.decode(tail, (address));
            // _update to address(0) burns instead of transferring, silently
            // dropping tokens HyperCore still counts as backed.
            if (recipient == address(0)) revert InvalidRecipient();
            _update(_systemAddress, recipient, amount);
        } else if (action == ACTION_INIT_TRANSFER) {
            uint128 amount128 = amount.toUint128();
            _update(_systemAddress, address(this), amount);
            _queueInitTransfer(from, coreNonce, amount128, tail);
        } else {
            revert UnknownAction(action);
        }

        emit CoreReceived(from, action, coreNonce, amount, data);
    }

    /// @dev Decodes eagerly so a malformed payload reverts while the transfer is
    /// still atomic with the HyperCore debit. The commitment itself lives on the
    /// bridge, keyed by `originNonce`: one contract to monitor instead of one per
    /// token, and this contract keeps no storage of its own for the flow.
    function _queueInitTransfer(
        address from,
        uint64 coreNonce,
        uint128 amount128,
        bytes calldata tail
    ) private {
        (uint128 fee, string memory recipient, string memory message) = abi
            .decode(tail, (uint128, string, string));

        IOmniBridgeQueueInitTransfer(owner()).queueInitTransfer(
            from,
            coreNonce,
            amount128,
            fee,
            recipient,
            message
        );
    }
}
