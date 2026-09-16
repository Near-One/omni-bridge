// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {OmniBridgeWormhole} from "./OmniBridgeWormhole.sol";
import {BridgeToken} from "./BridgeToken.sol";
import "./BridgeTypes.sol";

/// @notice OmniBridge variant for HyperEVM.
/// @dev The HyperCore -> HyperEVM callback runs as a system transaction whose logs
/// are absent from the block's `logsBloom`, so a message published from it is never
/// observed by the Wormhole guardians and never attested. HyperCore-originated
/// transfers are therefore committed by the token and submitted later from an
/// ordinary transaction. Only that path is split; ordinary `initTransfer` is
/// untouched.
// slither-disable-start unused-return
contract HlOmniBridgeWormhole is OmniBridgeWormhole {
    /// @custom:storage-location erc7201:aurora.HlOmniBridgeWormhole
    struct HlOmniBridgeWormholeStorage {
        mapping(uint64 => bytes32) pendingInitTransfers;
    }

    // keccak256(abi.encode(uint256(keccak256("aurora.HlOmniBridgeWormhole")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant HlOmniBridgeWormholeStorageLocation =
        0x7876e1b3b94b8102254e2bd34de4c744407920561311bf44bff59507404d2c00;

    function _getHlOmniBridgeWormholeStorage()
        private
        pure
        returns (HlOmniBridgeWormholeStorage storage $)
    {
        assembly {
            $.slot := HlOmniBridgeWormholeStorageLocation
        }
    }

    /// @notice originNonce => commitment; zero means nothing pending.
    function pendingInitTransfers(
        uint64 originNonce
    ) public view returns (bytes32) {
        return
            _getHlOmniBridgeWormholeStorage().pendingInitTransfers[originNonce];
    }

    event PreInitTransfer(
        uint64 indexed originNonce,
        address indexed tokenAddress,
        address indexed sender,
        uint64 coreNonce,
        uint128 amount,
        uint128 fee,
        string recipient,
        string message
    );

    error NoPendingInitTransfer(uint64 originNonce);
    error PayloadMismatch(uint64 originNonce);
    error NotBridgeToken(address caller);

    /// @notice Commits a HyperCore-originated transfer. Nothing is burned yet.
    /// @dev A revert here strands the tokens on HyperCore, so this step rejects as
    /// little as possible; the pause and the fee check are in the second step.
    function preInitTransfer(
        address sender,
        uint64 coreNonce,
        uint128 amount,
        uint128 fee,
        string calldata recipient,
        string calldata message
    ) external returns (uint64 originNonce) {
        if (!isBridgeToken[msg.sender]) {
            revert NotBridgeToken(msg.sender);
        }

        currentOriginNonce += 1;
        originNonce = currentOriginNonce;

        HlOmniBridgeWormholeStorage
            storage $ = _getHlOmniBridgeWormholeStorage();
        $.pendingInitTransfers[originNonce] = _initTransferCommitment(
            msg.sender,
            sender,
            amount,
            fee,
            recipient,
            message
        );

        emit PreInitTransfer(
            originNonce,
            msg.sender,
            sender,
            coreNonce,
            amount,
            fee,
            recipient,
            message
        );
    }

    /// @notice Submits a committed transfer. Permissionless; `payable` to cover the
    /// Wormhole message fee.
    function triggerPendingInitTransfer(
        uint64 originNonce,
        address tokenAddress,
        address sender,
        uint128 amount,
        uint128 fee,
        string calldata recipient,
        string calldata message
    ) external payable whenNotPaused(PAUSED_INIT_TRANSFER) {
        HlOmniBridgeWormholeStorage
            storage $ = _getHlOmniBridgeWormholeStorage();
        bytes32 committed = $.pendingInitTransfers[originNonce];
        if (committed == bytes32(0)) {
            revert NoPendingInitTransfer(originNonce);
        }
        if (
            committed !=
            _initTransferCommitment(
                tokenAddress,
                sender,
                amount,
                fee,
                recipient,
                message
            )
        ) {
            revert PayloadMismatch(originNonce);
        }
        if (fee >= amount) {
            revert InvalidFee();
        }

        delete $.pendingInitTransfers[originNonce];

        // The callback parked the tokens on the token contract itself.
        BridgeToken(tokenAddress).burn(tokenAddress, amount);

        initTransferExtension(
            sender,
            tokenAddress,
            originNonce,
            amount,
            fee,
            0,
            recipient,
            message,
            msg.value
        );

        emit BridgeTypes.InitTransfer(
            sender,
            tokenAddress,
            originNonce,
            amount,
            fee,
            0,
            recipient,
            message
        );
    }

    /// @dev `abi.encode`, not `encodePacked`: adjacent dynamic strings would let a
    /// packed encoding collide (`"ab" + "c"` vs `"a" + "bc"`).
    function _initTransferCommitment(
        address tokenAddress,
        address sender,
        uint128 amount,
        uint128 fee,
        string calldata recipient,
        string calldata message
    ) private pure returns (bytes32) {
        return
            keccak256(
                abi.encode(
                    tokenAddress,
                    sender,
                    amount,
                    fee,
                    recipient,
                    message
                )
            );
    }
}
// slither-disable-end unused-return
