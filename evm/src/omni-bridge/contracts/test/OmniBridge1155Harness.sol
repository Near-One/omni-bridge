// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

import "../OmniBridge.sol";

contract OmniBridge1155Harness is OmniBridge {
    function exposedGetOrCreateDeterministicAddress(address tokenAddress, uint256 tokenId)
        external
        returns (address)
    {
        return _getOrCreateDeterministicAddress(tokenAddress, tokenId);
    }

    function forceSetMultiToken(address deterministic, address tokenAddress, uint256 tokenId)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        MultiTokenInfo storage info = multiTokens[deterministic];
        info.tokenAddress = tokenAddress;
        info.tokenId = tokenId;
    }
}
