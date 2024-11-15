// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

library BridgeTypes {
    struct TransferMessagePayload {
        uint128 destination_nonce;
        uint8 origin_chain;
        uint128 origin_nonce;
        address tokenAddress;
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
        uint8 indexed origin_chain,
        uint128 indexed origin_nonce,
        address tokenAddress,
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

    event LogMetadata(
        address indexed tokenAddress,
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
