#!/usr/bin/env python3
"""
HyperLiquid sendToEvmWithData script

Transfers a token from HyperCore to an EVM contract with custom data payload.
The receiving contract must implement the ICoreReceiveWithData interface.

Usage:
  python3 hl_transfer.py \
    --token    "USDC:0x6d1e7cde53ba9467b783cb7c530ce054" \
    --amount   "10.0" \
    --to       0xRECIPIENT_CONTRACT \
    --chain-id 42161 \
    --data     0x

  Optional:
    --source-dex   ""         (default: "")
    --gas-limit    200000     (default: 200000)
    --encoding     hex        (default: "hex")
    --testnet                 (use testnet instead of mainnet)

Environment:
  PRIVATE_KEY — your wallet private key (required)

Install deps:
  pip install eth-account requests eth-utils
"""

import os
import sys
import time
import json
import argparse
import requests

from eth_account import Account
from eth_account.messages import encode_typed_data
from eth_utils import to_hex

API_URL_MAINNET = "https://api.hyperliquid.xyz/exchange"
API_URL_TESTNET = "https://api.hyperliquid-testnet.xyz/exchange"

SIGNATURE_CHAIN_ID = "0x66eee"

SEND_TO_EVM_WITH_DATA_TYPES = [
    {"name": "hyperliquidChain",    "type": "string"},
    {"name": "token",               "type": "string"},
    {"name": "amount",              "type": "string"},
    {"name": "sourceDex",           "type": "string"},
    {"name": "destinationRecipient","type": "string"},
    {"name": "addressEncoding",     "type": "string"},
    {"name": "destinationChainId",  "type": "uint32"},
    {"name": "gasLimit",            "type": "uint64"},
    {"name": "data",                "type": "bytes"},
    {"name": "nonce",               "type": "uint64"},
]

PRIMARY_TYPE = "HyperliquidTransaction:SendToEvmWithData"


def sign_inner(wallet, data: dict) -> dict:
    """Verbatim from SDK signing.py"""
    structured_data = encode_typed_data(full_message=data)
    signed = wallet.sign_message(structured_data)
    return {"r": to_hex(signed["r"]), "s": to_hex(signed["s"]), "v": signed["v"]}


def user_signed_payload(primary_type: str, payload_types: list, action: dict) -> dict:
    """Verbatim from SDK signing.py"""
    chain_id = int(action["signatureChainId"], 16)
    return {
        "domain": {
            "name": "HyperliquidSignTransaction",
            "version": "1",
            "chainId": chain_id,
            "verifyingContract": "0x0000000000000000000000000000000000000000",
        },
        "types": {
            primary_type: payload_types,
            "EIP712Domain": [
                {"name": "name",              "type": "string"},
                {"name": "version",           "type": "string"},
                {"name": "chainId",           "type": "uint256"},
                {"name": "verifyingContract", "type": "address"},
            ],
        },
        "primaryType": primary_type,
        "message": action,
    }


def sign_user_signed_action(wallet, action: dict, payload_types: list,
                            primary_type: str, is_mainnet: bool) -> dict:
    """Verbatim from SDK signing.py"""
    action["signatureChainId"] = SIGNATURE_CHAIN_ID
    action["hyperliquidChain"] = "Mainnet" if is_mainnet else "Testnet"
    data = user_signed_payload(primary_type, payload_types, action)
    return sign_inner(wallet, data)


def send_request(action: dict, signature: dict, nonce: int, is_mainnet: bool):
    url = API_URL_MAINNET if is_mainnet else API_URL_TESTNET
    payload = {
        "action":    action,
        "nonce":     nonce,
        "signature": signature,
    }
    print("\nRequest payload:")
    print(json.dumps(payload, indent=2, default=str))
    print(f"\nPOST {url}")

    resp = requests.post(url, json=payload, timeout=10)
    print(f"Response [{resp.status_code}]:")
    print(json.dumps(resp.json(), indent=2))
    return resp.json()


def main():
    parser = argparse.ArgumentParser(
        description="HyperLiquid sendToEvmWithData CLI",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--token",      required=True,
                        help='Token, e.g. "USDC:0x6d1e7cde53ba9467b783cb7c530ce054"')
    parser.add_argument("--amount",     required=True,
                        help='Amount as string, e.g. "10.0"')
    parser.add_argument("--to",         required=True,
                        help="Destination contract address (destinationRecipient)")
    parser.add_argument("--chain-id",   type=int, default=3,
                        help="Destination chain ID (default: 3 Arbitrum)")
    parser.add_argument("--data",       default="0x",
                        help="Custom data payload hex (default: 0x)")
    parser.add_argument("--source-dex", default="",
                        help="Source DEX name (default: \"\")")
    parser.add_argument("--gas-limit",  type=int, default=200000,
                        help="Gas limit for coreReceiveWithData call (default: 200000)")
    parser.add_argument("--encoding",   default="hex", choices=["hex", "base58"],
                        help="Address encoding (default: hex)")
    parser.add_argument("--testnet",    action="store_true",
                        help="Use testnet instead of mainnet")

    args = parser.parse_args()
    is_mainnet = not args.testnet

    private_key = os.environ.get("PRIVATE_KEY")
    if not private_key:
        print("Error: PRIVATE_KEY environment variable is not set")
        print("  export PRIVATE_KEY=0x...")
        sys.exit(1)

    wallet = Account.from_key(private_key)
    nonce  = int(time.time() * 1000)

    print(f"Wallet:      {wallet.address}")
    print(f"Network:     {'Mainnet' if is_mainnet else 'Testnet'}")
    print(f"Token:       {args.token}")
    print(f"Amount:      {args.amount}")
    print(f"Recipient:   {args.to}")
    print(f"Chain ID:    {args.chain_id}")
    print(f"Source DEX:  {args.source_dex}")
    print(f"Gas limit:   {args.gas_limit}")
    print(f"Data:        {args.data}")
    print(f"Nonce:       {nonce}")

    data_bytes = bytes.fromhex(args.data.removeprefix("0x"))

    action = {
        "type":                "sendToEvmWithData",
        "token":               args.token,
        "amount":              args.amount,
        "sourceDex":           args.source_dex,
        "destinationRecipient":args.to,
        "addressEncoding":     args.encoding,
        "destinationChainId":  args.chain_id,
        "gasLimit":            args.gas_limit,
        "data":                data_bytes,
        "nonce":               nonce,
    }

    signature = sign_user_signed_action(
        wallet, action, SEND_TO_EVM_WITH_DATA_TYPES, PRIMARY_TYPE, is_mainnet,
    )

    action["data"] = args.data

    send_request(action, signature, nonce, is_mainnet)


if __name__ == "__main__":
    main()
