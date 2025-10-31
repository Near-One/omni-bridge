import const
from const import (NearContract as NC, NearTestAccount as NTA)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

module btc_setup:
    snakefile: "./btc_setup.smk"
use rule * from btc_setup

# Directories
call_dir = const.common_generated_dir / "04-btc-transfer-{mode}"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

rule get_btc_user_deposit_address:
    wildcard_constraints:
        mode = "default"
    message: "Get BTC user deposit address"
    input:
        user_account_file = user_account_file
    output:
        call_dir / "01_btc_user_deposit_address.json"
    params:
        mkdir = lambda wc: get_mkdir_cmd(call_dir),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_connector_arg = "",
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    bridge-cli testnet get-bitcoin-address \
      --chain btc \
      {params.btc_connector_arg} \
      -r near:{params.user_account_id} \
      --amount 0 \
      --near-signer {params.user_account_id} \
      --config {params.bridge_sdk_config_file} \
      > {output}
    """

use rule get_btc_user_deposit_address as get_btc_user_deposit_address_test with:
    wildcard_constraints:
        mode = "test"
    input:
        rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file
    params:
        mkdir = lambda wc: get_mkdir_cmd(call_dir),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        btc_connector_arg = lambda wc, input: (
            f"--btc-connector {get_json_field(input.btc_connector_file, 'contract_id')}"
        ),


rule send_btc_to_deposit_address:
    message: "Send BTC to user deposit address on Bitcoin"
    input:
        step_1 =  call_dir / "01_btc_user_deposit_address.json",
    output: call_dir / "02_send_btc_to_deposit_address.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_address = lambda wc, input: get_btc_address(input.step_1),
    shell: """
    node {params.scripts_dir}/send_btc.js {params.btc_address} 7500 > {output}
    """

rule wait_tx:
    message: "Wait for BTC transaction"
    input:
        prev_step = call_dir / "02_send_btc_to_deposit_address.json",
    output: call_dir / "02_1_wait_tx.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_tx_hash = lambda wc, input: get_last_value(input.prev_step),
    shell: """
    node {params.scripts_dir}/wait_btc.js {params.btc_tx_hash} {output}
    """

rule fin_btc_transfer_on_near:
    wildcard_constraints:
        mode = "default"
    message: "Finalizing BTC transfer on Near"
    input:
        prev_step = call_dir / "02_1_wait_tx.json",
        step_2 = rules.send_btc_to_deposit_address.output,
        user_account_file = user_account_file
    output: call_dir / "03_fin_btc_transfer_on_near.json"
    params:
        btc_connector_arg = "",
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_2),
    shell: """
    bridge-cli testnet  near-fin-transfer-btc \
        --chain btc \
        -b {params.btc_tx_hash} \
        -v 0 \
        -r near:{params.user_account_id} \
        {params.btc_connector_arg} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

use rule fin_btc_transfer_on_near as fin_btc_transfer_on_near_test with:
    wildcard_constraints:
        mode = "test"
    input:
        prev_step = call_dir / "02_1_wait_tx.json",
        step_2 = rules.send_btc_to_deposit_address.output,
        user_account_file = user_account_file,
        nbtc_file = nbtc_file,
        btc_connector_file = btc_connector_file,
    params:
        btc_connector_arg = lambda wc, input: (
            f"--btc-connector {get_json_field(input.btc_connector_file, 'contract_id')}"
        ),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_2),


rule ft_transfer_btc_to_omni_bridge:
    wildcard_constraints:
        mode = "default"
    message: "Init BTC transfer to OmniBridge on Near"
    input:
        step_3 =  call_dir / "03_fin_btc_transfer_on_near.json",
        omni_bridge_storage_deposit = call_dir / "omni_bridge_storage_deposit.json",
        user_account_file = user_account_file,
    output: call_dir / "04_ft_transfer_btc_to_omni_bridge.json"
    params:
        scripts_dir = const.common_scripts_dir,
        nbtc_account = lambda wc, input: get_json_field(const.common_bridge_sdk_config_file, "btc"),
        omni_bridge_account = lambda wc, input: get_json_field(const.common_bridge_sdk_config_file, "near_token_locker_id"),
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.nbtc_account} \
        -m ft_transfer_call \
        -a '{{\"receiver_id\": \"{params.omni_bridge_account}\", \"amount\": \"5500\", \"msg\": \"{{\\\"recipient\\\": \\\"btc:tb1q4vvl8ykwprwv9dw3y5nrnpk7f2jech7atz45v5\\\", \\\"fee\\\":\\\"14\\\",\\\"native_token_fee\\\":\\\"162\\\"}}\"}}' \
        -f {input.user_account_file} \
        -d "1 yoctoNEAR" \
        -n testnet 2>&1 | tee {output} && \
        TX_HASH=$(grep -o 'Transaction ID: [^ ]*' {output} | cut -d' ' -f3) && \
        echo '{{\"tx_hash\": \"'$TX_HASH'\", \"contract_id\": \"{params.nbtc_account}\"}}' > {output}
    """

use rule ft_transfer_btc_to_omni_bridge as ft_transfer_btc_to_omni_bridge_test with:
    wildcard_constraints:
        mode = "test"
    input:
        step_3 =  call_dir / "03_fin_btc_transfer_on_near.json",
        omni_bridge_storage_deposit = call_dir / "omni_bridge_storage_deposit.json",
        user_account_file = user_account_file,
        add_utxo_chain = rules.add_utxo_chain_connector.output,
        nbtc_file = nbtc_file,
        omni_bridge_file = omni_bridge_file,
        btc_connector_file = btc_connector_file,
    params:
        scripts_dir = const.common_scripts_dir,
        nbtc_account = lambda wc, input: get_json_field(input.nbtc_file, "contract_id"),
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),


rule submit_transfer_to_btc_connector:
    message: "Sign BTC transfer on OmniBridge"
    input:
       step_7 = rules.ft_transfer_btc_to_omni_bridge.output,
       btc_connector_file = btc_connector_file,
       omni_bridge_file = omni_bridge_file,
       user_account_file = user_account_file
    output: call_dir / "05_sign_btc_transfer.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        near_tx_hash = lambda wc, input: get_json_field(input.step_7, "tx_hash"),

    shell: """
    bridge-cli testnet near-submit-btc-transfer \
        --chain btc \
        -n {params.near_tx_hash} \
        -s {params.user_account_id} \
        --near-token-locker-id {params.omni_bridge_account} \
        --btc-connector {params.btc_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule sign_btc_connector_transfer:
    message: "Sign BTC transfer on BtcConnector"
    input:
        add_utxo_chain = rules.add_utxo_chain_connector.output,
        step_7 = rules.submit_transfer_to_btc_connector.output,
        btc_connector_file = btc_connector_file,
        omni_bridge_file = omni_bridge_file,
        user_account_file = user_account_file
    output: call_dir / "06_sign_btc_connector_transfer.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_7),

    shell: """
    bridge-cli testnet near-sign-btc-transaction \
        --chain btc \
        --near-tx-hash {params.near_tx_hash} \
        --user-account {params.user_account_id} \
        --btc-connector {params.btc_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_btc_transfer:
    message: "Send BTC transfer"
    input:
        step_8 = rules.sign_btc_connector_transfer.output,
        user_account_file = user_account_file,
    output: call_dir / "07_send_btc_transfer.json"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_8),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
    --chain btc \
    --near-tx-hash {params.near_tx_hash} \
    --config {params.bridge_sdk_config_file} \
    > {output} \
    """

rule get_btc_tx_hash_from_logs:
    message: "Get BTC transaction hash from logs"
    input:
        prev_step = call_dir / "04_ft_transfer_btc_to_omni_bridge.json",
    output: call_dir / "btc_tx_hash_from_logs.json"
    params:
        scripts_dir = const.common_scripts_dir,
        near_tx_hash = lambda wc, input: get_json_field(input.prev_step, "tx_hash"),
    shell: """
    node {params.scripts_dir}/get_btc_tx_from_logs.js {params.near_tx_hash} {output}
    """

rule wait_final_tx:
    message: "Wait for Final BTC transaction"
    input:
        prev_step = call_dir / "btc_tx_hash_from_logs.json",
    output: call_dir / "wait_final_tx.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_tx_hash = lambda wc, input: get_json_field(input.prev_step, "btc_pending_sign_id"),
    shell: """
    node {params.scripts_dir}/wait_btc.js {params.btc_tx_hash} {output}
    """

rule check_bridge_finalised:
    message: "Check if Bridge transfer is finalised"
    input:
        prev_step = call_dir / "wait_final_tx.json",
        near_tx_hash_step = call_dir / "04_ft_transfer_btc_to_omni_bridge.json",
    output:
        call_dir / "bridge_finalised"
    params:
        bridge_api = "https://testnet.api.bridge.nearone.org/api/v2/transfers/transfer",
        near_tx_hash = lambda wc, input: get_json_field(input.near_tx_hash_step, "tx_hash"),
    shell: r"""
    TX_HASH="{params.near_tx_hash}"
    API="{params.bridge_api}?transaction_hash=${{TX_HASH}}"

    RESP=$(curl -s "$API")

    FINAL=$(echo "$RESP" | jq -r '.[0].finalised')

    if [ "$FINAL" = "null" ] || [ -z "$FINAL" ]; then
        echo "Bridge transfer is NOT finalised for $TX_HASH" >&2
        exit 1
    else
        echo "BTC TX IS FINALISED!" > {output}
    fi
    """

rule all:
    input:
        const.common_generated_dir / "04-btc-transfer-test" / "07_send_btc_transfer.json",
    default_target: True

rule btc_transfer_default_contracts:
    input:
        const.common_generated_dir / "04-btc-transfer-default" / "bridge_finalised",
