// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import {AccessControlUpgradeable} from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

import "./BridgeToken.sol";
import "./SelectivePausableUpgradable.sol";
import "./Borsh.sol";

contract BridgeTokenFactory is
    UUPSUpgradeable,
    AccessControlUpgradeable,
    SelectivePausableUpgradable
{
    enum WhitelistMode {
        NotInitialized,
        Blocked,
        CheckToken,
        CheckAccountAndToken
    }

    // We removed ProofConsumer from the list of parent contracts and added this gap
    // to preserve storage layout when upgrading to the new contract version.
    uint256[54] private __gap;

    mapping(address => string) private _ethToNearToken;
    mapping(string => address) private _nearToEthToken;
    mapping(address => bool) private _isBridgeToken;

    mapping(string => WhitelistMode) private _whitelistedTokens;
    mapping(bytes => bool) private _whitelistedAccounts;
    bool private _isWhitelistModeEnabled;

    address public tokenImplementationAddress;
    address public nearBridgeDerivedAddress;

    mapping(uint128 => bool) public completedTransfers;
    uint128 public initTransferNonce; 

    bytes32 public constant PAUSABLE_ADMIN_ROLE = keccak256("PAUSABLE_ADMIN_ROLE");
    uint constant UNPAUSED_ALL = 0;
    uint constant PAUSED_INIT_TRANSFER = 1 << 0;
    uint constant PAUSED_FIN_TRANSFER = 1 << 1;

    struct FinTransferPayload {
        uint128 nonce;
        string token;
        uint128 amount;
        address recipient;
        string feeRecipient;
    }

    struct MetadataPayload {
        string token;
        string name;
        string symbol;
        uint8 decimals;
    }

    event InitTransfer(
        address indexed sender,
        address indexed tokenAddress,
        uint128 indexed nonce,
        string token,
        uint128 amount,
        uint128 fee,
        string recipient
    );


    event FinTransfer(
        uint128 indexed nonce,
        string token,
        uint128 amount,
        address recipient,
        string feeRecipient
    );

    event DeployToken(
        address indexed tokenAddress,
        string token,
        string name,
        string symbol,
        uint8 decimals
    );

    event SetMetadata(
        address indexed tokenAddress,
        string token,
        string name,
        string symbol,
        uint8 decimals
    );

    error InvalidSignature();
    error NonceAlreadyUsed(uint256 nonce);

    function initialize(
        address _tokenImplementationAddress,
        address _nearBridgeDerivedAddress
    ) public initializer {
        tokenImplementationAddress = _tokenImplementationAddress;
        nearBridgeDerivedAddress = _nearBridgeDerivedAddress;

        __UUPSUpgradeable_init();
        __AccessControl_init();
        __Pausable_init_unchained();
        _grantRole(DEFAULT_ADMIN_ROLE, _msgSender());
        _grantRole(PAUSABLE_ADMIN_ROLE, _msgSender());
    }

    function isBridgeToken(address token) external view returns (bool) {
        return _isBridgeToken[token];
    }

    function ethToNearToken(address token) external view returns (string memory) {
        require(_isBridgeToken[token], "ERR_NOT_BRIDGE_TOKEN");
        return _ethToNearToken[token];
    }

    function nearToEthToken(string calldata nearTokenId) external view returns (address) {
        require(_isBridgeToken[_nearToEthToken[nearTokenId]], "ERR_NOT_BRIDGE_TOKEN");
        return _nearToEthToken[nearTokenId];
    }

    function newBridgeToken(bytes calldata signatureData, MetadataPayload calldata metadata) payable external returns (address) {
        bytes memory borshEncoded = bytes.concat(
            Borsh.encodeString(metadata.token),
            Borsh.encodeString(metadata.name),
            Borsh.encodeString(metadata.symbol),
            bytes1(metadata.decimals)
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        require(!_isBridgeToken[_nearToEthToken[metadata.token]], "ERR_TOKEN_EXIST");

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

        emit DeployToken(
            bridgeTokenProxy,
            metadata.token,
            metadata.name,
            metadata.symbol,
            metadata.decimals
        );

        _isBridgeToken[address(bridgeTokenProxy)] = true;
        _ethToNearToken[address(bridgeTokenProxy)] = metadata.token;
        _nearToEthToken[metadata.token] = address(bridgeTokenProxy);

        return bridgeTokenProxy;
    }

    function deployTokenExtension(string memory token, address tokenAddress) internal virtual {}

    function setMetadata(
        string calldata token,
        string calldata name,
        string calldata symbol
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(_isBridgeToken[_nearToEthToken[token]], "ERR_NOT_BRIDGE_TOKEN");

        BridgeToken bridgeToken = BridgeToken(_nearToEthToken[token]);

        bridgeToken.setMetadata(name, symbol, bridgeToken.decimals());

        emit SetMetadata(
            address(bridgeToken),
            token,
            name,
            symbol,
            bridgeToken.decimals()
        );
    }

    function finTransfer(bytes calldata signatureData, FinTransferPayload calldata payload) payable external whenNotPaused(PAUSED_FIN_TRANSFER) {
        if (completedTransfers[payload.nonce]) {
            revert NonceAlreadyUsed(payload.nonce);
        }

        bytes memory borshEncoded = bytes.concat(
            Borsh.encodeUint128(payload.nonce),
            Borsh.encodeString(payload.token),
            Borsh.encodeUint128(payload.amount),
            bytes1(0x00), // variant 1 in rust enum
            Borsh.encodeAddress(payload.recipient),
            bytes(payload.feeRecipient).length == 0  // None or Some(String) in rust
                ? bytes("\x00") 
                : bytes.concat(bytes("\x01"), Borsh.encodeString(payload.feeRecipient))
        );
        bytes32 hashed = keccak256(borshEncoded);

        if (ECDSA.recover(hashed, signatureData) != nearBridgeDerivedAddress) {
            revert InvalidSignature();
        }

        require(_isBridgeToken[_nearToEthToken[payload.token]], "ERR_NOT_BRIDGE_TOKEN");
        BridgeToken(_nearToEthToken[payload.token]).mint(payload.recipient, payload.amount);

        completedTransfers[payload.nonce] = true;

        finTransferExtension(payload);

        emit FinTransfer(
            payload.nonce,
            payload.token,
            payload.amount,
            payload.recipient,
            payload.feeRecipient
        );
    }

    function finTransferExtension(FinTransferPayload memory payload) internal virtual {}

    function initTransfer(
        string calldata token,
        uint128 amount,
        uint128 fee,
        string calldata recipient
    ) payable external whenNotPaused(PAUSED_INIT_TRANSFER) {
        initTransferNonce += 1;
        _checkWhitelistedToken(token, msg.sender);
        require(_isBridgeToken[_nearToEthToken[token]], "ERR_NOT_BRIDGE_TOKEN");

        address tokenAddress = _nearToEthToken[token];

        BridgeToken(tokenAddress).burn(msg.sender, amount + fee);

        initTransferExtension(initTransferNonce, token, amount, fee, recipient, msg.sender);

        emit InitTransfer(msg.sender, tokenAddress, initTransferNonce, token , amount, fee, recipient);
    }

    function initTransferExtension(
        uint128 nonce,
        string calldata token,
        uint128 amount,
        uint128 fee,
        string calldata recipient,
        address sender
    ) internal virtual {}

    function pause(uint flags) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _pause(flags);
    }

    function pauseFinTransfer() external onlyRole(PAUSABLE_ADMIN_ROLE) {
        _pause(pausedFlags() | PAUSED_FIN_TRANSFER);
    }

    function pauseInitTransfer() external onlyRole(PAUSABLE_ADMIN_ROLE) {
        _pause(pausedFlags() | PAUSED_INIT_TRANSFER);
    }

    function pauseAll() external onlyRole(PAUSABLE_ADMIN_ROLE) {
        uint flags = PAUSED_FIN_TRANSFER | PAUSED_INIT_TRANSFER;
        _pause(flags);
    }

    function isWhitelistModeEnabled() external view returns (bool) {
        return _isWhitelistModeEnabled;
    }

    function getTokenWhitelistMode(
        string calldata token
    ) external view returns (WhitelistMode) {
        return _whitelistedTokens[token];
    }

    function isAccountWhitelistedForToken(
        string calldata token,
        address account
    ) external view returns (bool) {
        return _whitelistedAccounts[abi.encodePacked(token, account)];
    }

    function upgradeToken(
        string calldata nearTokenId,
        address implementation
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(_isBridgeToken[_nearToEthToken[nearTokenId]], "ERR_NOT_BRIDGE_TOKEN");
        BridgeToken proxy = BridgeToken(payable(_nearToEthToken[nearTokenId]));
        proxy.upgradeToAndCall(implementation, bytes(""));
    }

    function enableWhitelistMode() external onlyRole(DEFAULT_ADMIN_ROLE) {
        _isWhitelistModeEnabled = true;
    }

    function disableWhitelistMode() external onlyRole(DEFAULT_ADMIN_ROLE) {
        _isWhitelistModeEnabled = false;
    }

    function setTokenWhitelistMode(
        string calldata token,
        WhitelistMode mode
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _whitelistedTokens[token] = mode;
    }

    function addAccountToWhitelist(
        string calldata token,
        address account
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(
            _whitelistedTokens[token] != WhitelistMode.NotInitialized,
            "ERR_NOT_INITIALIZED_WHITELIST_TOKEN"
        );
        _whitelistedAccounts[abi.encodePacked(token, account)] = true;
    }

    function removeAccountFromWhitelist(
        string calldata token,
        address account
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        delete _whitelistedAccounts[abi.encodePacked(token, account)];
    }

    function _checkWhitelistedToken(string memory token, address account) internal view {
        if (!_isWhitelistModeEnabled) {
            return;
        }

        WhitelistMode tokenMode = _whitelistedTokens[token];
        require(
            tokenMode != WhitelistMode.NotInitialized,
            "ERR_NOT_INITIALIZED_WHITELIST_TOKEN"
        );
        require(tokenMode != WhitelistMode.Blocked, "ERR_WHITELIST_TOKEN_BLOCKED");

        if (tokenMode == WhitelistMode.CheckAccountAndToken) {
            require(
                _whitelistedAccounts[abi.encodePacked(token, account)],
                "ERR_ACCOUNT_NOT_IN_WHITELIST"
            );
        }
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
