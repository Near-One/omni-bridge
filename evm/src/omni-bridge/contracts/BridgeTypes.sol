// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.8.24;

library BridgeTypes {
    struct TransferMessagePayload {
        uint64 destinationNonce;
        uint8 originChain;
        uint64 originNonce;
        address tokenAddress;
        uint128 amount;
        address recipient;
        string feeRecipient;
        string message;
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
        uint64 indexed originNonce,
        uint128 amount,
        uint128 fee,
        uint128 nativeFee,
        string recipient,
        string message
    );

    event FinTransfer(
        uint8 indexed originChain,
        uint64 indexed originNonce,
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
        uint8 decimals,
        uint8 originDecimals
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
