import json
import os
from typing import TypedDict, Literal

import requests
from dotenv import load_dotenv
from eth_account import Account
from eth_account.signers.local import LocalAccount
from hyperliquid.utils import constants
from hyperliquid.utils.signing import get_timestamp_ms, sign_l1_action

# Load .env from the same directory as this script.
load_dotenv(os.path.join(os.path.dirname(__file__), ".env"))

LINK_PARAMS_PATH = os.path.join(os.path.dirname(__file__), "link_tokens_params.json")

# Type def for the finalize action (we only support the firstStorageSlot mode).
FinalizeEvmContractAction = TypedDict(
    "FinalizeEvmContractAction",
    {
        "type": Literal["finalizeEvmContract"],
        "token": int,
        "input": Literal["firstStorageSlot"],
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
        raise RuntimeError(
            "HL_SECRET_KEY is not set. Add it to evm/utils/hyperliquid/.env (see .env.example)."
        )
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


def show_state(base_url, user_address):
    """Print spotDeployState for the user — useful to inspect pending requestEvmContract."""
    response = requests.post(
        base_url + "/info",
        json={"type": "spotDeployState", "user": user_address},
    )
    print("\n=== spotDeployState ===")
    print(json.dumps(response.json(), indent=2))


def requestEvmContract(account, base_url, params):
    """Step 1 of linking: declare which EVM contract should be linked to the HC token."""
    action = {
        "type": "spotDeploy",
        "requestEvmContract": {
            "token": params["token_id"],
            "address": params["evm_contract_address"].lower(),
            "evmExtraWeiDecimals": params["evm_extra_wei_decimals"],
        },
    }
    nonce = get_timestamp_ms()
    signature = sign_l1_action(account, action, None, nonce, None, False)
    payload = {
        "action": action,
        "nonce": nonce,
        "signature": signature,
        "vaultAddress": None,
    }
    response = requests.post(base_url + "/exchange", json=payload)
    print(response.json())


def finalizeEvmContract(account, base_url, params):
    """Step 2 of linking: prove ownership of the EVM contract so HL activates the link.

    Uses the "firstStorageSlot" verification mode: HL queries storage slot 0 of the
    EVM contract and expects it to contain the signer's address. The EVM contract
    therefore must have the signer's address in slot 0 — verify this before running.
    """
    finalize_action: FinalizeEvmContractAction = {
        "type": "finalizeEvmContract",
        "token": params["token_id"],
        "input": "firstStorageSlot",
    }
    nonce = get_timestamp_ms()
    signature = sign_l1_action(account, finalize_action, None, nonce, None, False)
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

    # Sanity checks — fail early with a clear message if config is incomplete.
    if params.get("token_id") is None:
        raise RuntimeError("token_id is not set in link_tokens_params.json")
    if params.get("evm_contract_address") is None:
        raise RuntimeError("evm_contract_address is not set in link_tokens_params.json")

    base_url = get_base_url(params["network"])
    account: LocalAccount = Account.from_key(get_secret_key())
    print(f"Running with address {account.address}")
    print(
        f"Linking HC token {params['token_id']} → EVM contract {params['evm_contract_address']} "
        f"(network={params['network']}, mode=firstStorageSlot)"
    )

    # --- requestEvmContract: reversible, always run without confirm ---
    requestEvmContract(account, base_url, params)

    # --- finalizeEvmContract: IRREVERSIBLE, confirm + sanity-check pending state ---
    # Show state so we can verify the pending request actually matches what
    # we're about to finalize.
    show_state(base_url, account.address)
    if not confirm(
        f"Run finalizeEvmContract? "
        f"IRREVERSIBLE: locks token {params['token_id']} ↔ EVM contract {params['evm_contract_address']}"
    ):
        return
    finalizeEvmContract(account, base_url, params)


if __name__ == "__main__":
    main()
