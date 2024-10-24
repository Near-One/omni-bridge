// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {AccessControlUpgradeable} from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

import "./BridgeToken.sol";
import "./SelectivePausableUpgradable.sol";
import "./Borsh.sol";
import "./BridgeTypes.sol";

contract BridgeTokenFactory is
    UUPSUpgradeable,
    AccessControlUpgradeable,
    SelectivePausableUpgradable
{
    mapping(address => string) public ethToNearToken;
    mapping(string => address) public nearToEthToken;
    mapping(address => bool) public isBridgeToken;

    address public tokenImplementationAddress;
    address public nearBridgeDerivedAddress;
    uint8 public omniBridgeChainId;

    mapping(uint128 => bool) public completedTransfers;
    mapping(uint128 => bool) public claimedFee;
    uint128 public initTransferNonce; 

    bytes32 public constant PAUSABLE_ADMIN_ROLE = keccak256("PAUSABLE_ADMIN_ROLE");
    uint constant UNPAUSED_ALL = 0;
    uint constant PAUSED_INIT_TRANSFER = 1 << 0;
    uint constant PAUSED_FIN_TRANSFER = 1 << 1;

    error InvalidSignature();
    error NonceAlreadyUsed(uint256 nonce);
    error InvalidFee();

    function initialize(
        address tokenImplementationAddress_,
        address nearBridgeDerivedAddress_,
        uint8 omniBridgeChainId_
    ) public initializer {
        tokenImplementationAddress = tokenImplementationAddress_;
        nearBridgeDerivedAddress = nearBridgeDerivedAddress_;
        omniBridgeChainId = omniBridgeChainId_;

        __UUPSUpgradeable_init();
        __AccessControl_init();
        __Pausable_init_unchained();
        _grantRole(DEFAULT_ADMIN_ROLE, _msgSender());
        _grantRole(PAUSABLE_ADMIN_ROLE, _msgSender());
    }

    function deployToken(bytes calldata signatureData, BridgeTypes.MetadataPayload calldata metadata) payable external returns (address) {
        bytes memory borshEncoded = bytes.concat(
            bytes1(uint8(BridgeTypes.PayloadType.Metadata)),
            Borsh.encodeString(metadata.token),
            Borsh.encodeString(metadata.name),
            Borsh.encodeString(metadata.symbol),
            bytes1(metadata.decimals)
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        require(!isBridgeToken[nearToEthToken[metadata.token]], "ERR_TOKEN_EXIST");

        address bridgeTokenProxy = address(
            new ERC1967Proxy(
                tokenImplementationAddress,
                abi.encodeWithSelector(
                    BridgeToken.initialize.selector,
                    metadata.name,
                    metadata.symbol,
                    metadata.decimals
                )
            )
        );

        deployTokenExtension(metadata.token, bridgeTokenProxy);

        emit BridgeTypes.DeployToken(
            bridgeTokenProxy,
            metadata.token,
            metadata.name,
            metadata.symbol,
            metadata.decimals
        );

        isBridgeToken[address(bridgeTokenProxy)] = true;
        ethToNearToken[address(bridgeTokenProxy)] = metadata.token;
        nearToEthToken[metadata.token] = address(bridgeTokenProxy);

        return bridgeTokenProxy;
    }

    function deployTokenExtension(string memory token, address tokenAddress) internal virtual {}

    function setMetadata(
        string calldata token,
        string calldata name,
        string calldata symbol
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(isBridgeToken[nearToEthToken[token]], "ERR_NOT_BRIDGE_TOKEN");

        BridgeToken bridgeToken = BridgeToken(nearToEthToken[token]);

        bridgeToken.setMetadata(name, symbol, bridgeToken.decimals());

        emit BridgeTypes.SetMetadata(
            address(bridgeToken),
            token,
            name,
            symbol,
            bridgeToken.decimals()
        );
    }

    function finTransfer(bytes calldata signatureData, BridgeTypes.FinTransferPayload calldata payload) payable external whenNotPaused(PAUSED_FIN_TRANSFER) {
        if (completedTransfers[payload.nonce]) {
            revert NonceAlreadyUsed(payload.nonce);
        }

        bytes memory borshEncoded = bytes.concat(
            bytes1(uint8(BridgeTypes.PayloadType.TransferMessage)),
            Borsh.encodeUint128(payload.nonce),
            Borsh.encodeString(payload.token),
            Borsh.encodeUint128(payload.amount),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(payload.recipient),
            bytes(payload.feeRecipient).length == 0  // None or Some(String) in rust
                ? bytes("\x00") 
                : bytes.concat(bytes("\x01"), Borsh.encodeString(payload.feeRecipient))
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        require(isBridgeToken[nearToEthToken[payload.token]], "ERR_NOT_BRIDGE_TOKEN");
        BridgeToken(nearToEthToken[payload.token]).mint(payload.recipient, payload.amount);

        completedTransfers[payload.nonce] = true;

        finTransferExtension(payload);

        emit BridgeTypes.FinTransfer(
            payload.nonce,
            payload.token,
            payload.amount,
            payload.recipient,
            payload.feeRecipient
        );
    }

    function finTransferExtension(BridgeTypes.FinTransferPayload memory payload) internal virtual {}

    function initTransfer(
        string calldata token,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient
    ) payable external whenNotPaused(PAUSED_INIT_TRANSFER) {
        initTransferNonce += 1;
        require(isBridgeToken[nearToEthToken[token]], "ERR_NOT_BRIDGE_TOKEN");
        if (fee >= amount) {
            revert InvalidFee();
        }

        address tokenAddress = nearToEthToken[token];

        BridgeToken(tokenAddress).burn(msg.sender, amount);

        uint256 extensionValue = msg.value - nativeFee;
        initTransferExtension(initTransferNonce, token, amount, fee, nativeFee, recipient, msg.sender, extensionValue);

        emit BridgeTypes.InitTransfer(msg.sender, tokenAddress, initTransferNonce, token , amount, fee, nativeFee, recipient);
    }

    function claimNativeFee(bytes calldata signatureData, BridgeTypes.ClaimFeePayload memory payload) external {
        bytes memory borshEncodedNonces = Borsh.encodeUint32(uint32(payload.nonces.length));

        for (uint i = 0; i < payload.nonces.length; ++i) {
            uint128 nonce = payload.nonces[i];
            if (claimedFee[nonce]) {
                revert NonceAlreadyUsed(nonce);
            }

            claimedFee[nonce] = true;
            borshEncodedNonces = bytes.concat(
            bytes1(uint8(BridgeTypes.PayloadType.ClaimNativeFee)),
                borshEncodedNonces,
                Borsh.encodeUint128(nonce)
            );
        }        
        
        bytes memory borshEncoded = bytes.concat(
            borshEncodedNonces,
            Borsh.encodeUint128(payload.amount),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(payload.recipient)
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        (bool success,) = payload.recipient.call{value: payload.amount}("");
        require(success, "Failed to send Ether.");
    }

    function initTransferExtension(
        uint128 nonce,
        string calldata token,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        address sender,
        uint256 value
    ) internal virtual {}

    function pause(uint flags) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _pause(flags);
    }

    function pauseAll() external onlyRole(PAUSABLE_ADMIN_ROLE) {
        uint flags = PAUSED_FIN_TRANSFER | PAUSED_INIT_TRANSFER;
        _pause(flags);
    }

    function upgradeToken(
        string calldata nearTokenId,
        address implementation
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(isBridgeToken[nearToEthToken[nearTokenId]], "ERR_NOT_BRIDGE_TOKEN");
        BridgeToken proxy = BridgeToken(payable(nearToEthToken[nearTokenId]));
        proxy.upgradeToAndCall(implementation, bytes(""));
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
