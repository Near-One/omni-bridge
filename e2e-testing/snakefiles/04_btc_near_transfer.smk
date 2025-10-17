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
call_dir = const.common_generated_dir / "04-btc-near-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

rule get_btc_user_deposit_address:
    message: "Get BTC user deposit address"
    input:
        rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file
    output: call_dir / "01_btc_user_deposit_address.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file
    shell: """
    {params.mkdir} && \
         bridge-cli testnet get-bitcoin-address \
         --chain btc \
         --btc-connector {params.btc_connector} \
         --amount 0 \
         -r near:{params.user_account_id} \
         --near-signer {params.user_account_id} \
         --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_btc_to_deposit_address:
    message: "Send BTC to user deposit address on Bitcoin"
    input:
        step_1 = rules.get_btc_user_deposit_address.output,
    output: call_dir / "02_send_btc_to_deposit_address.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_address = lambda wc, input: get_btc_address(input.step_1),
    shell: """
    node {params.scripts_dir}/send_btc.js {params.btc_address} 7500 > {output}
    """

rule fin_btc_transfer_on_near:
    message: "Finalizing BTC transfer on Near"
    input:
        step_2 = rules.send_btc_to_deposit_address.output,
        nbtc_file = nbtc_file,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file
    output: call_dir / "03_fin_btc_transfer_on_near.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
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
        --btc-connector {params.btc_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule btc_near_transfer_all:
    input:
        rules.fin_btc_transfer_on_near.output,
    default_target: True
