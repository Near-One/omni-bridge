from typing import TypedDict, Literal, Union

import requests
from eth_account import Account
from eth_account.signers.local import LocalAccount
from web3 import Web3
from web3.middleware import SignAndSendRawMiddlewareBuilder
from hyperliquid.utils import constants
from hyperliquid.utils.signing import get_timestamp_ms, sign_l1_action

CreateInputParams = TypedDict("CreateInputParams", {"nonce": int})
CreateInput = TypedDict("CreateInput", {"create": CreateInputParams})
FinalizeEvmContractInput = Union[Literal["firstStorageSlot"], CreateInput]
FinalizeEvmContractAction = TypedDict(
    "FinalizeEvmContractAction",
    {"type": Literal["finalizeEvmContract"], "token": int, "input": FinalizeEvmContractInput},
)

DEFAULT_CONTRACT_ADDRESS = Web3.to_checksum_address(
    "0x2E98e98aB34b42b14FeC9d431F7B051B232Ba133"  # change this to your contract address if you are skipping deploying
)
TOKEN = 1562  # note that if changing this you likely should also change the abi to have a different name and perhaps also different decimals and initial supply
PRIVATE_KEY = "0xPRIVATE_KEY"  # Change this to your private key

# Connect to the JSON-RPC endpoint
rpc_url = "https://rpc.hyperliquid-testnet.xyz/evm"

contract_address = DEFAULT_CONTRACT_ADDRESS

def requestEvmContract(account):
    assert contract_address is not None
    action = {
        "type": "spotDeploy",
        "requestEvmContract": {
            "token": TOKEN,
            "address": contract_address.lower(),
            "evmExtraWeiDecimals": 10,
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
    response = requests.post(constants.TESTNET_API_URL + "/exchange", json=payload)
    print(response.json())

def finalizeEvmContract(account):
    creation_nonce = 4
    print(creation_nonce)
    use_create_finalization = True
    finalize_action: FinalizeEvmContractAction
    if use_create_finalization:
        finalize_action = {
            "type": "finalizeEvmContract",
            "token": TOKEN,
            "input": {"create": {"nonce": creation_nonce}},
        }
    else:
        finalize_action = {"type": "finalizeEvmContract", "token": TOKEN, "input": "firstStorageSlot"}
    nonce = get_timestamp_ms()
    signature = sign_l1_action(account, finalize_action, None, nonce, None, False)
    payload = {
        "action": finalize_action,
        "nonce": nonce,
        "signature": signature,
        "vaultAddress": None,
    }
    response = requests.post(constants.TESTNET_API_URL + "/exchange", json=payload)
    print(response.json())


def main():
    w3 = Web3(Web3.HTTPProvider(rpc_url))

    # The account will be used both for deploying the ERC20 contract and linking it to your native spot asset
    # You can also switch this to create an account a different way if you don't want to include a secret key in code
    if PRIVATE_KEY == "0xPRIVATE_KEY":
        raise Exception("must set private key or create account another way")
    account: LocalAccount = Account.from_key(PRIVATE_KEY)
    print(f"Running with address {account.address}")
    w3.middleware_onion.add(SignAndSendRawMiddlewareBuilder.build(account))
    w3.eth.default_account = account.address
    # Verify connection
    if not w3.is_connected():
        raise Exception("Failed to connect to the Ethereum network")

    print(TOKEN, contract_address.lower())
    #requestEvmContract(account)
    finalizeEvmContract(account)

if __name__ == "__main__":
    main()

# curl -s https://api.hyperliquid-testnet.xyz/info   -H "Content-Type: application/json"   -d '{"type": "tokenDetails", "tokenId": "0x646586ef3576346a4fcc9548909c1cba"}' | jq
# curl -s https://api.hyperliquid-testnet.xyz/info  -H "Content-Type: application/json"  -d '{ "type": "spotMeta" }' | jq '.tokens[] | select(.name=="JHWL")'
