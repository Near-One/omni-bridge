# Hyperliquid utils

Scripts and helpers for interacting with Hyperliquid HyperCore (HC) and HyperEVM.

## Setup

### 1. Create a Python environment

Recommended: a dedicated conda env to isolate dependencies from the rest of the project.

```bash
conda create -n hl-utils python=3.11 -y
conda activate hl-utils
```

(or use `venv` / `pyenv` if you prefer — any Python 3.10+ works)

### 2. Install dependencies

Install all three via `pip` (the conda env is just for Python isolation):

```bash
pip install hyperliquid-python-sdk python-dotenv eth-account requests
```

- `hyperliquid-python-sdk` — official Python client (gives `hyperliquid.exchange`, `hyperliquid.info`, `hyperliquid.utils`)
- `python-dotenv` — loads secrets from `.env`
- `eth-account` — Ethereum account / signing primitives (usually pulled in transitively, but we depend on it directly)
- `requests` — HTTP client used by `links_tokens.py` to call `/exchange` and `/info` endpoints directly

> Note: `python-dotenv` and `eth-account` are available on `conda-forge`, but `hyperliquid-python-sdk` is **only** on PyPI. Mixing conda + pip in one env can occasionally break dependency resolution — easier to install all three with pip inside the activated conda env.

### 3. Create a fresh agent wallet and approve it on Hyperliquid

We never sign with the token owner's private key directly. Instead, create a brand-new EVM key pair that will act as an **API (agent) wallet** for the owner account, and approve it through the Hyperliquid UI:

1. Generate a fresh EVM key locally (any tool that gives you `(address, private key)` — e.g. `cast wallet new`, MetaMask "Create account", or a one-off Python snippet with `eth_account`).
2. Open <https://app.hyperliquid.xyz/API> while logged in as the **token owner** (the master account).
3. Add the fresh address as a new API wallet (give it a name like `deploy-script`).
4. Confirm the approval transaction from the master account.

The agent wallet does not hold funds — it only signs trading-style actions on behalf of the master. The fresh private key is what you will put into `.env` as `HL_SECRET_KEY` in the next step.

> Note: agents are restricted to trading actions. Admin / deploy actions (`spotDeploy.*`, `finalizeEvmContract`, etc.) must still be signed by the master key directly. For those one-off operations you would temporarily put the master key into `.env`; for everyday operations the agent key is enough and safer.

### 4. Configure secrets — `.env`

Copy the example and fill in your values:

```bash
cp .env.example .env
$EDITOR .env
```

Required:

- `HL_SECRET_KEY` — private key of your HyperCore deployer (hex, with or without `0x`).
- `HL_ACCOUNT_ADDRESS` — leave empty if the secret key is your main account's. Set it only if `HL_SECRET_KEY` is an API/agent wallet and you want to act on behalf of a different main account.

⚠️ Never commit `.env`. It's already covered by `.gitignore` patterns for env files in the repo.

### 5. Configure deployment parameters — `deploy_params.json`

This file is **not secret** and is meant to be committed. Edit it before running:

| Field | Meaning |
|---|---|
| `network` | `"testnet"` or `"mainnet"` |
| `token_id` | `null` initially. After step1 succeeds the script writes the returned token index here. |
| `spot_id` | `null` initially. After step4 succeeds the script writes the returned spot index here. |
| `last_step` | `null` initially. Updated after each step; controls resume logic. |
| `token_name` | HIP-1 token name (e.g. `"NEAR"`) — immutable after step1. |
| `token_description` | Free-form description. |
| `sz_decimals` | Trading-precision decimals (typically 2). |
| `wei_decimals` | Atomic-unit decimals (typically 8). |
| `max_gas` | Max HYPE willing to pay in the spot-deploy auction, in HYPE wei (1 HYPE = 10^8 wei). |
| `total_supply` | Genesis supply allocation, as a decimal string in atomic units. We use `2^64 - 1` to set the maximum HyperCore-representable cap (bridged tokens only ever circulate what is actually bridged in). |
| `start_px` | Reference price for `register_hyperliquidity` (anchor price). With `noHyperliquidity = True` it's recorded but no orders are placed. |

### 6. Configure link parameters — `link_tokens_params.json`

Separate non-secret config consumed by `links_tokens.py`. Fields:

| Field | Meaning |
|---|---|
| `network` | `"testnet"` or `"mainnet"` (independent of `deploy_params.json`'s `network`) |
| `token_id` | HC token index to link (the same number `spot_deploy.py` writes into `deploy_params.json` after step 1) |
| `evm_contract_address` | Address of the ERC-20 contract on HyperEVM that should be linked to `token_id` |
| `evm_extra_wei_decimals` | Additional EVM-side decimals on top of `wei_decimals` (HC `wei_decimals` + `evm_extra_wei_decimals` = ERC-20 `decimals()`). Typical: `10` |
| `last_link_step` | `null` initially. Reserved for future skip / resume logic (not yet enforced) |

## Scripts

### `spot_deploy.py`

End-to-end HIP-1 token deploy on HyperCore (steps 1–5).

Run:

```bash
python spot_deploy.py
```

The script:

1. Loads `.env` and `deploy_params.json`.
2. Connects to the network (`HL_NETWORK` from params).
3. Walks through steps 1-5, **printing `spotDeployState` from HL before each** and **asking for `[y/N]` confirmation**.
4. After each successful step, persists progress back to `deploy_params.json` (`last_step`, `token_id`, `spot_id`).

Step skip-rules (so resume / partial runs are safe):

| Step | Skipped if |
|---|---|
| 1. register_token | `last_step >= 1` **OR** `token_id` set |
| 2. user_genesis | `last_step >= 2` **OR** `spot_id` set |
| 3. genesis | `last_step >= 3` **OR** `spot_id` set |
| 4. register_spot | `last_step >= 4` **OR** `spot_id` set |
| 5. register_hyperliquidity | `last_step >= 5` |

Replying `n` to a confirm prompt exits cleanly — progress is preserved, you can re-run later.

### `links_tokens.py`

Links a HIP-1 HC token to an existing ERC-20 contract on HyperEVM. Uses the **`firstStorageSlot` verification mode**: HL reads storage slot 0 of the EVM contract and expects it to contain the signer's address.

Run:

```bash
python links_tokens.py
```

## Notes

### Reversibility (read before running on mainnet)

| Step | Reversible? | Notes |
|---|---|---|
| 1. register_token | ❌ if auction won | Costs HYPE. Name/decimals immutable after success. Re-running with `token_id = null` will register a *new* token. |
| 2. user_genesis | ⚠️ until step 3 | Multiple calls allowed, but frozen after `genesis`. |
| 3. genesis | ❌ | Locks `max_supply` and `noHyperliquidity` permanently. |
| 4. register_spot | ❌ | Initial pair is `<token>/USDC`. Other quote tokens can be added later via a separate permissionless Dutch auction. |
| 5. register_hyperliquidity | ❌ | With `noHyperliquidity = True` it's a formality (`n_orders = 0`), but still must be called. |

### Hardcoded design choices (in code, not config)

- `noHyperliquidity = True` (step 3) — we're deploying a bridged token, all liquidity comes from the bridge, never from a protocol-level AMM.
- `order_sz = 0`, `n_orders = 0`, `n_seeded_levels = None` (step 5) — forced by the choice above.
- Initial spot pair quote token = USDC (index 0) — required by HL at initial deployment.

### Account-mode requirement

Some deploy actions (especially admin-style ones like `setDeployerTradingFeeShare`, `enableQuoteToken`) require the deployer account to be in **Standard / Manual mode**, not Unified Account / Portfolio Margin. If a call returns `Action disabled when unified account is active`, disable Portfolio Margin and Unified Account Mode in the HL UI (settings, top-right) and retry.
