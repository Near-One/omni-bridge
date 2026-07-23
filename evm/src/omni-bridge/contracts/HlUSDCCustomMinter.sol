// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {AccessControlUpgradeable} from "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {ICustomMinter} from "../../common/ICustomMinter.sol";

// ──────────────────────────────────────────────────────────────────────────────
// HyperLiquid USDC CustomMinter for OmniBridge
//
// On HyperLiquid, USDC on HyperEVM is a standard ERC-20 (Circle's native USDC),
// but actual trading liquidity lives on HyperCore. Circle provides
// CoreDepositWallet — a bridge contract that moves USDC from HyperEVM → HyperCore.
//
// Flow:
//   - finTransfer (NEAR → HyperLiquid, "mint"):
//       This contract holds a USDC liquidity pool on HyperEVM.
//       On mint, it approves CoreDepositWallet and calls depositFor(recipient),
//       which transfers USDC from this contract to the recipient on HyperCore.
//   - initTransfer (HyperLiquid → NEAR, "burn"):
//       OmniBridge transfers USDC from the user to this contract.
//       burn is a no-op — we cannot burn Circle's USDC.
//       The received USDC replenishes the liquidity pool.
//
// Circle's CoreDepositWallet:
//   https://github.com/circlefin/hyperevm-circle-contracts/blob/master/src/CoreDepositWallet.sol
//   depositFor(address recipient, uint256 amount, uint32 destinationDex)
//     - calls transferFrom(msg.sender, ...), so we approve first
//     - destinationDex: 0 = Perps, type(uint32).max = Spot
//
// HyperLiquid contract addresses:
//   Mainnet (chainId 999):
//     USDC (native, ERC-20):  0xb88339CB7199b77E23DB6E890353E22632Ba630f
//     CoreDepositWallet:      0x6b9e773128f453f5c2c60935ee2de2cbc5390a24
//   Testnet (chainId 998):
//     USDC (native, ERC-20):  0x2B3370eE501B4a559b57D449569354196457D8Ab
//     CoreDepositWallet:      0x0b80659a4076e9e93c7dbe0f10675a16a3e5c206
//
// Setup:
//   1. Deploy HlUSDCCustomMinter (proxy)
//   2. OmniBridge admin calls addCustomToken(nearTokenId, USDC, HlUSDCCustomMinter, 6)
//   3. MINTER_ROLE is granted to OmniBridge during initialize()
//   4. Fund this contract with USDC (it holds liquidity for finTransfer)
// ──────────────────────────────────────────────────────────────────────────────

interface ICoreDepositWallet {
    function depositFor(
        address recipient,
        uint256 amount,
        uint32 destinationDex
    ) external;
}

contract HlUSDCCustomMinter is
    UUPSUpgradeable,
    AccessControlUpgradeable,
    ICustomMinter
{
    using SafeERC20 for IERC20;

    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");

    // Spot dex on HyperCore
    uint32 public constant DESTINATION_DEX_SPOT = type(uint32).max;

    address public usdc;
    address public coreDepositWallet;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address _usdc,
        address _coreDepositWallet,
        address _admin,
        address _omniBridge
    ) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();

        usdc = _usdc;
        coreDepositWallet = _coreDepositWallet;
        _grantRole(DEFAULT_ADMIN_ROLE, _admin);
        _grantRole(MINTER_ROLE, _omniBridge);
    }

    /// @notice Called by OmniBridge during finTransfer.
    ///         Approves USDC to CoreDepositWallet and calls depositFor
    ///         to credit the recipient on HyperCore.
    function mint(
        address token,
        address to,
        uint128 amount
    ) external onlyRole(MINTER_ROLE) {
        require(token == usdc, "USDCCustomMinter: wrong token");

        IERC20(usdc).forceApprove(coreDepositWallet, uint256(amount));
        ICoreDepositWallet(coreDepositWallet).depositFor(
            to,
            uint256(amount),
            DESTINATION_DEX_SPOT
        );
    }

    /// @notice Called by OmniBridge during initTransfer.
    ///         OmniBridge transfers USDC from the user to this contract first,
    ///         then calls burn. We simply hold the USDC as liquidity — no actual burn.
    function burn(
        address token,
        uint128 /* amount */
    ) external onlyRole(MINTER_ROLE) {
        require(token == usdc, "USDCCustomMinter: wrong token");
        // no-op: USDC stays in this contract as liquidity
    }

    function setCoreDepositWallet(
        address _coreDepositWallet
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        coreDepositWallet = _coreDepositWallet;
    }

    function _authorizeUpgrade(
        address newImplementation
    ) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
