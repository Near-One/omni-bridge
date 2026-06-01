import os

import eth_account
from dotenv import load_dotenv
from eth_account.signers.local import LocalAccount

from hyperliquid.exchange import Exchange
from hyperliquid.info import Info
from hyperliquid.utils import constants

# Load .env from the same directory as this script (not the cwd).
load_dotenv(os.path.join(os.path.dirname(__file__), ".env"))


def setup(base_url=None, skip_ws=False, perp_dexs=None):
    secret_key = get_secret_key()
    account: LocalAccount = eth_account.Account.from_key(secret_key)
    address = os.environ.get("HL_ACCOUNT_ADDRESS", "") or account.address
    print("Running with account address:", address)
    if address != account.address:
        print("Running with agent address:", account.address)
    info = Info(base_url, skip_ws, perp_dexs=perp_dexs)
    user_state = info.user_state(address)
    spot_user_state = info.spot_user_state(address)
    margin_summary = user_state["marginSummary"]
    if float(margin_summary["accountValue"]) == 0 and len(spot_user_state["balances"]) == 0:
        print("Not running the example because the provided account has no equity.")
        url = info.base_url.split(".", 1)[1]
        error_string = f"No accountValue:\nIf you think this is a mistake, make sure that {address} has a balance on {url}.\nIf address shown is your API wallet address, set HL_ACCOUNT_ADDRESS in .env to the address of your main account (not the API wallet)."
        raise Exception(error_string)
    exchange = Exchange(account, base_url, account_address=address, perp_dexs=perp_dexs)
    return address, info, exchange


def get_secret_key():
    secret_key = os.environ.get("HL_SECRET_KEY")
    if not secret_key:
        raise RuntimeError(
            "HL_SECRET_KEY is not set. Add it to evm/utils/hyperliquid/.env (see .env.example)."
        )
    return secret_key


def get_base_url():
    network = os.environ.get("HL_NETWORK", "testnet").lower()
    if network == "mainnet":
        return constants.MAINNET_API_URL
    if network == "testnet":
        return constants.TESTNET_API_URL
    raise RuntimeError(
        f"Invalid HL_NETWORK={network!r}. Expected 'testnet' or 'mainnet'."
    )


def step1(exchange):
    # Step 1: Registering the Token
    #
    # Takes part in the spot deploy auction and if successful, registers token "TEST0"
    # with sz_decimals 2 and wei_decimals 8.
    # The max gas is 10,000 HYPE and represents the max amount to be paid for the spot deploy auction.
    register_token_result = exchange.spot_deploy_register_token("TEST0", 2, 8, 1000000000000, "Test token example")
    print(register_token_result)
    # If registration is successful, a token index will be returned. This token index is required for
    # later steps in the spot deploy process.
    if register_token_result["status"] == "ok":
        token = register_token_result["response"]["data"]
    else:
        return
    return token

def step2(address, exchange, token):
    # Step 2: User Genesis
    #
    # User genesis can be called multiple times to associate balances to specific users and/or
    # tokens for genesis.
    user_genesis_result = exchange.spot_deploy_user_genesis(
        token,
        [
            (address, "100000000900000000"),
        ],
        [],
    )
    print(user_genesis_result)

def step3(exchange, token):
    # Step 3: Genesis
    #
    # Finalize genesis. The max supply of 300000000000000 wei needs to match the total
    # allocation above from user genesis.
    #
    # "noHyperliquidity" can also be set to disable hyperliquidity. In that case, no balance
    # should be associated with hyperliquidity from step 2 (user genesis).
    genesis_result = exchange.spot_deploy_genesis(token, "100000000900000000", True)
    print(genesis_result)

def step4(exchange, token):
    # Step 4: Register Spot
    #
    # Register the spot pair (TEST0/USDC) given base and quote token indices. 0 represents USDC.
    # The base token is the first token in the pair and the quote token is the second token.
    register_spot_result = exchange.spot_deploy_register_spot(token, 0)
    print(register_spot_result)
    # If registration is successful, a spot index will be returned. This spot index is required for
    # registering hyperliquidity.
    if register_spot_result["status"] == "ok":
        spot = register_spot_result["response"]["data"]
    else:
        return

    return spot

def step5(exchange, spot):

    # Step 5: Register Hyperliquidity
    #
    # Registers hyperliquidity for the spot pair. In this example, hyperliquidity is registered
    # with a starting price of $2, an order size of 4, and 100 total orders.
    #
    # This step is required even if "noHyperliquidity" was set to True.
    # If "noHyperliquidity" was set to True during step 3 (genesis), then "n_orders" is required to be 0.
    register_hyperliquidity_result = exchange.spot_deploy_register_hyperliquidity(spot, 2.0, 4.0, 0, None)
    print(register_hyperliquidity_result)

def main():
    address, info, exchange = setup(get_base_url(), skip_ws=True)
    print(address, info, exchange)

    # token = step1()
    # token = 1562
    # step2(address, exchange, token)
    # step3(exchange, token)
    # spot = step4(exchange, token)
    # print(spot)
    # spot = 1436
    # step5(exchange, spot)

if __name__ == "__main__":
    main()
