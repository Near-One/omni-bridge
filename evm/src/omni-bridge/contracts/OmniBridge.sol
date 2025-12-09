// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {AccessControlUpgradeable} from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import {IERC1155} from "@openzeppelin/contracts/token/ERC1155/IERC1155.sol";
import {IERC1155Receiver} from "@openzeppelin/contracts/token/ERC1155/IERC1155Receiver.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {ICustomMinter} from "../../common/ICustomMinter.sol";

import "./BridgeToken.sol";
import "./SelectivePausableUpgradable.sol";
import "../../common/Borsh.sol";
import "./BridgeTypes.sol";

contract OmniBridge is UUPSUpgradeable, AccessControlUpgradeable, SelectivePausableUpgradable, IERC1155Receiver {
    using SafeERC20 for IERC20;

    struct MultiTokenInfo {
        address tokenAddress;
        uint256 tokenId;
    }

    mapping(address => string) public ethToNearToken;
    mapping(string => address) public nearToEthToken;
    mapping(address => bool) public isBridgeToken;
    mapping(address => MultiTokenInfo) public multiTokens;

    address public tokenImplementationAddress;
    address public nearBridgeDerivedAddress;
    uint8 public omniBridgeChainId;

    mapping(uint64 => bool) public completedTransfers;
    uint64 public currentOriginNonce;

    mapping(address => address) public customMinters;

    bytes32 public constant PAUSABLE_ADMIN_ROLE = keccak256("PAUSABLE_ADMIN_ROLE");
    uint256 constant UNPAUSED_ALL = 0;
    uint256 constant PAUSED_INIT_TRANSFER = 1 << 0;
    uint256 constant PAUSED_FIN_TRANSFER = 1 << 1;

    error InvalidSignature();
    error NonceAlreadyUsed(uint64 nonce);
    error InvalidFee();
    error InvalidValue();
    error FailedToSendEther();
    error ERC1155MappingMismatch();
    error ERC1155DirectSendNotAllowed();
    error ERC1155BatchNotSupported();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

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

    function addCustomToken(
        string calldata nearTokenId,
        address tokenAddress,
        address customMinter,
        uint8 originDecimals
    ) external payable onlyRole(DEFAULT_ADMIN_ROLE) {
        isBridgeToken[tokenAddress] = true;
        ethToNearToken[tokenAddress] = nearTokenId;
        nearToEthToken[nearTokenId] = tokenAddress;
        customMinters[tokenAddress] = customMinter;

        string memory name = IERC20Metadata(tokenAddress).name();
        string memory symbol = IERC20Metadata(tokenAddress).symbol();
        uint8 decimals = IERC20Metadata(tokenAddress).decimals();

        deployTokenExtension(nearTokenId, tokenAddress, decimals, originDecimals);

        emit BridgeTypes.DeployToken(tokenAddress, nearTokenId, name, symbol, decimals, originDecimals);
    }

    function removeCustomToken(address tokenAddress) external onlyRole(DEFAULT_ADMIN_ROLE) {
        delete isBridgeToken[tokenAddress];
        delete nearToEthToken[ethToNearToken[tokenAddress]];
        delete ethToNearToken[tokenAddress];
        delete customMinters[tokenAddress];
    }

    function acceptTokenOwnership(address tokenAddress) external onlyRole(DEFAULT_ADMIN_ROLE) {
        BridgeToken(tokenAddress).acceptOwnership();
    }

    function deployToken(bytes calldata signatureData, BridgeTypes.MetadataPayload calldata metadata)
        external
        payable
        returns (address)
    {
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
        uint8 decimals = _normalizeDecimals(metadata.decimals);

        // slither-disable-next-line reentrancy-no-eth
        address bridgeTokenProxy = address(
            new ERC1967Proxy(
                tokenImplementationAddress,
                abi.encodeWithSelector(BridgeToken.initialize.selector, metadata.name, metadata.symbol, decimals)
            )
        );

        deployTokenExtension(metadata.token, bridgeTokenProxy, decimals, metadata.decimals);

        emit BridgeTypes.DeployToken(
            bridgeTokenProxy, metadata.token, metadata.name, metadata.symbol, decimals, metadata.decimals
        );

        isBridgeToken[address(bridgeTokenProxy)] = true;
        ethToNearToken[address(bridgeTokenProxy)] = metadata.token;
        nearToEthToken[metadata.token] = address(bridgeTokenProxy);

        return bridgeTokenProxy;
    }

    function deployTokenExtension(string memory token, address tokenAddress, uint8 decimals, uint8 originDecimals)
        internal
        virtual
    {}

    function setMetadata(string calldata token, string calldata name, string calldata symbol)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        require(isBridgeToken[nearToEthToken[token]], "ERR_NOT_BRIDGE_TOKEN");

        BridgeToken bridgeToken = BridgeToken(nearToEthToken[token]);

        bridgeToken.setMetadata(name, symbol, bridgeToken.decimals());

        emit BridgeTypes.SetMetadata(address(bridgeToken), token, name, symbol, bridgeToken.decimals());
    }

    function logMetadata(address tokenAddress) external payable {
        string memory name = IERC20Metadata(tokenAddress).name();
        string memory symbol = IERC20Metadata(tokenAddress).symbol();
        uint8 decimals = IERC20Metadata(tokenAddress).decimals();

        logMetadataExtension(tokenAddress, name, symbol, decimals);

        emit BridgeTypes.LogMetadata(tokenAddress, name, symbol, decimals);
    }

    function logMetadata1155(address tokenAddress, uint256 tokenId) external payable {
        address deterministicToken = _getOrCreateDeterministicAddress(tokenAddress, tokenId);

        logMetadataExtension(deterministicToken, "", "", 0);

        emit BridgeTypes.LogMetadata(deterministicToken, "", "", 0);
    }

    function logMetadataExtension(address tokenAddress, string memory name, string memory symbol, uint8 decimals)
        internal
        virtual
    {}

    function finTransfer(bytes calldata signatureData, BridgeTypes.TransferMessagePayload calldata payload)
        external
        payable
        whenNotPaused(PAUSED_FIN_TRANSFER)
    {
        if (completedTransfers[payload.destinationNonce]) {
            revert NonceAlreadyUsed(payload.destinationNonce);
        }

        completedTransfers[payload.destinationNonce] = true;

        bytes memory borshEncoded = bytes.concat(
            bytes1(uint8(BridgeTypes.PayloadType.TransferMessage)),
            Borsh.encodeUint64(payload.destinationNonce),
            bytes1(payload.originChain),
            Borsh.encodeUint64(payload.originNonce),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(payload.tokenAddress),
            Borsh.encodeUint128(payload.amount),
            bytes1(omniBridgeChainId),
            Borsh.encodeAddress(payload.recipient),
            bytes(payload.feeRecipient).length == 0 // None or Some(String) in rust
                ? bytes("\x00")
                : bytes.concat(bytes("\x01"), Borsh.encodeString(payload.feeRecipient))
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        MultiTokenInfo memory multiToken = multiTokens[payload.tokenAddress];

        if (payload.tokenAddress == address(0)) {
            // slither-disable-next-line arbitrary-send-eth
            (bool success,) = payload.recipient.call{value: payload.amount}("");
            if (!success) revert FailedToSendEther();
        } else if (multiToken.tokenAddress != address(0)) {
            IERC1155(multiToken.tokenAddress).safeTransferFrom(
                address(this), payload.recipient, multiToken.tokenId, payload.amount, ""
            );
        } else if (customMinters[payload.tokenAddress] != address(0)) {
            ICustomMinter(customMinters[payload.tokenAddress]).mint(
                payload.tokenAddress, payload.recipient, payload.amount
            );
        } else if (isBridgeToken[payload.tokenAddress]) {
            BridgeToken(payload.tokenAddress).mint(payload.recipient, payload.amount);
        } else {
            IERC20(payload.tokenAddress).safeTransfer(payload.recipient, payload.amount);
        }

        finTransferExtension(payload);

        emit BridgeTypes.FinTransfer(
            payload.originChain,
            payload.originNonce,
            payload.tokenAddress,
            payload.amount,
            payload.recipient,
            payload.feeRecipient
        );
    }

    function finTransferExtension(BridgeTypes.TransferMessagePayload memory payload) internal virtual {}

    function initTransfer(
        address tokenAddress,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message
    ) external payable whenNotPaused(PAUSED_INIT_TRANSFER) {
        currentOriginNonce += 1;
        if (fee >= amount) {
            revert InvalidFee();
        }

        uint256 extensionValue;
        if (tokenAddress == address(0)) {
            if (fee != 0) {
                revert InvalidFee();
            }
            extensionValue = msg.value - amount - nativeFee;
        } else {
            extensionValue = msg.value - nativeFee;
            if (customMinters[tokenAddress] != address(0)) {
                IERC20(tokenAddress).safeTransferFrom(msg.sender, customMinters[tokenAddress], amount);
                ICustomMinter(customMinters[tokenAddress]).burn(tokenAddress, amount);
            } else if (isBridgeToken[tokenAddress]) {
                BridgeToken(tokenAddress).burn(msg.sender, amount);
            } else {
                IERC20(tokenAddress).safeTransferFrom(msg.sender, address(this), amount);
            }
        }

        initTransferExtension(
            msg.sender, tokenAddress, currentOriginNonce, amount, fee, nativeFee, recipient, message, extensionValue
        );

        emit BridgeTypes.InitTransfer(
            msg.sender, tokenAddress, currentOriginNonce, amount, fee, nativeFee, recipient, message
        );
    }

    function initTransfer1155(
        address tokenAddress,
        uint256 tokenId,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string calldata recipient,
        string calldata message
    ) external payable whenNotPaused(PAUSED_INIT_TRANSFER) {
        currentOriginNonce += 1;
        if (fee >= amount) {
            revert InvalidFee();
        }

        address deterministicToken = _getOrCreateDeterministicAddress(tokenAddress, tokenId);

        IERC1155(tokenAddress).safeTransferFrom(msg.sender, address(this), tokenId, amount, "");

        uint256 extensionValue = msg.value - nativeFee;

        initTransferExtension(
            msg.sender,
            deterministicToken,
            currentOriginNonce,
            amount,
            fee,
            nativeFee,
            recipient,
            message,
            extensionValue
        );

        emit BridgeTypes.InitTransfer(
            msg.sender, deterministicToken, currentOriginNonce, amount, fee, nativeFee, recipient, message
        );
    }

    function initTransferExtension(
        address, /*sender*/
        address, /*tokenAddress*/
        uint64, /*originNonce*/
        uint128, /*amount*/
        uint128, /*fee*/
        uint128, /*nativeFee*/
        string calldata, /*recipient*/
        string calldata, /*message*/
        uint256 value
    ) internal virtual {
        if (value != 0) {
            revert InvalidValue();
        }
    }

    // We intentionally avoid advertising IERC1155Receiver support so tooling does not suggest direct ERC1155 sends.
    // Only transfers initiated by this contract itself are accepted.
    function supportsInterface(bytes4 interfaceId)
        public
        view
        virtual
        override(AccessControlUpgradeable, IERC165)
        returns (bool)
    {
        return super.supportsInterface(interfaceId);
    }

    function onERC1155Received(address operator, address, uint256, uint256, bytes calldata)
        external
        view
        override
        returns (bytes4)
    {
        // Only accept transfers that were initiated by this contract itself
        if (operator != address(this)) {
            revert ERC1155DirectSendNotAllowed();
        }

        return this.onERC1155Received.selector;
    }

    function onERC1155BatchReceived(address, address, uint256[] calldata, uint256[] calldata, bytes calldata)
        external
        pure
        override
        returns (bytes4)
    {
        // Explicitly reject batched multi-token transfers
        revert ERC1155BatchNotSupported();
    }

    function pause(uint256 flags) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _pause(flags);
    }

    function pauseAll() external onlyRole(PAUSABLE_ADMIN_ROLE) {
        uint256 flags = PAUSED_FIN_TRANSFER | PAUSED_INIT_TRANSFER;
        _pause(flags);
    }

    function upgradeToken(address tokenAddress, address implementation) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(isBridgeToken[tokenAddress], "ERR_NOT_BRIDGE_TOKEN");
        BridgeToken proxy = BridgeToken(tokenAddress);
        proxy.upgradeToAndCall(implementation, bytes(""));
    }

    function setNearBridgeDerivedAddress(address nearBridgeDerivedAddress_) external onlyRole(DEFAULT_ADMIN_ROLE) {
        nearBridgeDerivedAddress = nearBridgeDerivedAddress_;
    }

    receive() external payable {}

    function deriveDeterministicAddress(address tokenAddress, uint256 tokenId) public pure returns (address) {
        uint160 addr160 = uint160(tokenAddress);
        uint32 prefix = uint32(addr160 >> 128);

        uint256 h = uint256(keccak256(abi.encodePacked(tokenAddress, tokenId)));
        uint128 suffix = uint128(h >> 128);

        uint160 result = (uint160(prefix) << 128) | uint160(suffix);

        return address(result);
    }

    function _getOrCreateDeterministicAddress(address tokenAddress, uint256 tokenId) internal returns (address) {
        address deterministic = deriveDeterministicAddress(tokenAddress, tokenId);

        MultiTokenInfo storage multiToken = multiTokens[deterministic];

        if (multiToken.tokenAddress == address(0)) {
            multiToken.tokenAddress = tokenAddress;
            multiToken.tokenId = tokenId;
        } else {
            if (multiToken.tokenAddress != tokenAddress || multiToken.tokenId != tokenId) {
                revert ERC1155MappingMismatch();
            }
        }

        return deterministic;
    }

    function _normalizeDecimals(uint8 decimals) internal pure returns (uint8) {
        uint8 maxAllowedDecimals = 18;
        if (decimals > maxAllowedDecimals) {
            return maxAllowedDecimals;
        }
        return decimals;
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}

    uint256[49] private __gap;
}
