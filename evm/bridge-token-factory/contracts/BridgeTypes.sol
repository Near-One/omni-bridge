// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

library BridgeTypes {
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

    struct ClaimFeePayload {
        uint128[] nonces;
        uint128 amount;
        address recipient;
    }

    event InitTransfer(
        address indexed sender,
        address indexed tokenAddress,
        uint128 indexed nonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string recipient,
        string message
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

    enum PayloadType {
        TransferMessage,
        Metadata,
        ClaimNativeFee
    }
}
