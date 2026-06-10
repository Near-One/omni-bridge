import json
import os
from typing import TypedDict, Literal

import requests
from dotenv import load_dotenv
from eth_account import Account
from eth_account.signers.local import LocalAccount
from hyperliquid.utils import constants
from hyperliquid.utils.signing import get_timestamp_ms, sign_l1_action

load_dotenv(os.path.join(os.path.dirname(__file__), ".env"))

LINK_PARAMS_PATH = os.path.join(os.path.dirname(__file__), "link_tokens_params.json")

FinalizeEvmContractAction = TypedDict(
    "FinalizeEvmContractAction",
    {
        "type": Literal["finalizeEvmContract"],
        "token": int,
        "input": Literal["customStorageSlot"],
    },
)


def load_params():
    with open(LINK_PARAMS_PATH) as f:
        return json.load(f)


def save_params(params):
    with open(LINK_PARAMS_PATH, "w") as f:
        json.dump(params, f, indent=2)
        f.write("\n")


def get_secret_key():
    secret_key = os.environ.get("HL_SECRET_KEY")
    if not secret_key:
        raise RuntimeError("HL_SECRET_KEY is not set. See .env.example.")
    return secret_key


def get_evm_secret_key():
    secret_key = os.environ.get("EVM_SECRET_KEY")
    if not secret_key:
        raise RuntimeError("EVM_SECRET_KEY is not set. See .env.example.")
    return secret_key


def get_base_url(network):
    network = network.lower()
    if network == "mainnet":
        return constants.MAINNET_API_URL
    if network == "testnet":
        return constants.TESTNET_API_URL
    raise RuntimeError(f"Invalid network={network!r}. Expected 'testnet' or 'mainnet'.")


def confirm(prompt):
    return input(f"\n{prompt} [y/N]: ").strip().lower() == "y"


def show_state(base_url, token_index):
    """Print spotMeta entry + tokenDetails for the token."""
    meta = requests.post(base_url + "/info", json={"type": "spotMeta"}).json()
    token = next(
        (t for t in meta.get("tokens", []) if t.get("index") == token_index),
        None,
    )
    if token is None:
        print(
            f"\n[show_state] token index {token_index} not found in spotMeta "
            f"— token may not be deployed yet on this network"
        )
        return

    token_id_hex = token["tokenId"]

    details = requests.post(
        base_url + "/info",
        json={"type": "tokenDetails", "tokenId": token_id_hex},
    ).json()

    print("\n=== spotMeta entry ===")
    print(json.dumps(token, indent=2))
    print("\n=== tokenDetails ===")
    print(json.dumps(details, indent=2))


def requestEvmContract(account, base_url, params):
    """Step 1: declare which EVM contract should be linked to the HC token."""
    action = {
        "type": "spotDeploy",
        "requestEvmContract": {
            "token": params["token_id"],
            "address": params["evm_contract_address"].lower(),
            "evmExtraWeiDecimals": params["evm_extra_wei_decimals"],
        },
    }
    nonce = get_timestamp_ms()
    is_mainnet = base_url == constants.MAINNET_API_URL
    signature = sign_l1_action(account, action, None, nonce, None, is_mainnet)
    payload = {
        "action": action,
        "nonce": nonce,
        "signature": signature,
        "vaultAddress": None,
    }
    response = requests.post(base_url + "/exchange", json=payload)
    print(response.json())


def finalizeEvmContract(account, base_url, params):
    """Step 2: signer's address must match slot keccak256('HyperCore deployer')
    in the EVM contract (see `setHyperCoreDeployer` in `HlBridgeToken.sol`).
    """
    finalize_action: FinalizeEvmContractAction = {
        "type": "finalizeEvmContract",
        "token": params["token_id"],
        "input": "customStorageSlot",
    }
    nonce = get_timestamp_ms()
    is_mainnet = base_url == constants.MAINNET_API_URL
    signature = sign_l1_action(account, finalize_action, None, nonce, None, is_mainnet)
    payload = {
        "action": finalize_action,
        "nonce": nonce,
        "signature": signature,
        "vaultAddress": None,
    }
    response = requests.post(base_url + "/exchange", json=payload)
    print(response.json())


def main():
    params = load_params()

    if params.get("token_id") is None:
        raise RuntimeError("token_id is not set in link_tokens_params.json")
    if params.get("evm_contract_address") is None:
        raise RuntimeError("evm_contract_address is not set in link_tokens_params.json")

    base_url = get_base_url(params["network"])
    hl_account: LocalAccount = Account.from_key(get_secret_key())
    evm_account: LocalAccount = Account.from_key(get_evm_secret_key())

    print(f"HL signer (step 1, requestEvmContract):  {hl_account.address}")
    print(f"EVM signer (step 2, finalizeEvmContract): {evm_account.address}")
    print(
        f"Linking HC token {params['token_id']} → EVM contract {params['evm_contract_address']} "
        f"(network={params['network']}, mode=customStorageSlot)"
    )

    requestEvmContract(hl_account, base_url, params)

    show_state(base_url, params["token_id"])
    if not confirm(
        f"Run finalizeEvmContract? "
        f"IRREVERSIBLE: locks token {params['token_id']} ↔ EVM contract {params['evm_contract_address']}"
    ):
        return
    finalizeEvmContract(evm_account, base_url, params)


if __name__ == "__main__":
    main()
