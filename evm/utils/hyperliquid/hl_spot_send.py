#!/usr/bin/env python3
"""
HyperLiquid spotSend — free spot token transfer on HyperCore.

Uses the EXACT same signing code as the SDK (copied verbatim from signing.py).
If this works (returns correct wallet address in error), the signing setup is
correct and the issue is only in the EIP-712 types for sendToEvmWithData.

Usage:
  python3 hl_spot_send.py \
    --token  "USDC:0x6d1e7cde53ba9467b783cb7c530ce054" \
    --amount "1.0" \
    --to     0xRECIPIENT

  Optional:
    --testnet   (use testnet instead of mainnet)

Environment:
  PRIVATE_KEY — your wallet private key (required)
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

# Copied verbatim from SDK signing.py
SPOT_TRANSFER_SIGN_TYPES = [
    {"name": "hyperliquidChain", "type": "string"},
    {"name": "destination",      "type": "string"},
    {"name": "token",            "type": "string"},
    {"name": "amount",           "type": "string"},
    {"name": "time",             "type": "uint64"},
]

PRIMARY_TYPE = "HyperliquidTransaction:SpotSend"


def sign_inner(wallet, data: dict) -> dict:
    """Copied verbatim from SDK signing.py"""
    structured_data = encode_typed_data(full_message=data)
    signed = wallet.sign_message(structured_data)
    return {"r": to_hex(signed["r"]), "s": to_hex(signed["s"]), "v": signed["v"]}


def user_signed_payload(primary_type: str, payload_types: list, action: dict) -> dict:
    """Copied verbatim from SDK signing.py"""
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
    """Copied verbatim from SDK signing.py"""
    action["signatureChainId"] = "0x66eee"
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
    parser = argparse.ArgumentParser(description="HyperLiquid spotSend CLI")
    parser.add_argument("--token",   required=True,
                        help='Token, e.g. "USDC:0x6d1e7cde53ba9467b783cb7c530ce054"')
    parser.add_argument("--amount",  required=True,
                        help='Amount as string, e.g. "1.0"')
    parser.add_argument("--to",      required=True,
                        help="Destination address")
    parser.add_argument("--testnet", action="store_true",
                        help="Use testnet instead of mainnet")
    args = parser.parse_args()

    is_mainnet = not args.testnet

    private_key = os.environ.get("PRIVATE_KEY")
    if not private_key:
        print("Error: PRIVATE_KEY environment variable is not set")
        sys.exit(1)

    wallet = Account.from_key(private_key)
    nonce  = int(time.time() * 1000)

    print(f"Wallet:  {wallet.address}")
    print(f"Network: {'Mainnet' if is_mainnet else 'Testnet'}")
    print(f"Token:   {args.token}")
    print(f"Amount:  {args.amount}")
    print(f"To:      {args.to}")
    print(f"Nonce:   {nonce}")

    action = {
        "type":        "spotSend",
        "destination": args.to,
        "token":       args.token,
        "amount":      args.amount,
        "time":        nonce,
    }

    signature = sign_user_signed_action(
        wallet, action, SPOT_TRANSFER_SIGN_TYPES, PRIMARY_TYPE, is_mainnet,
    )

    send_request(action, signature, nonce, is_mainnet)


if __name__ == "__main__":
    main()
