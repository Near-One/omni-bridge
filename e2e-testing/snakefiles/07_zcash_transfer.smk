import pathlib
import const
import time
from const import (get_evm_deploy_results_dir, get_evm_account_dir,
                     EvmNetwork as EN, NearContract as NC, EvmContract as EC, NearTestAccount as NTA, NearExternalContract as NEC)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash, get_zcash_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

module zcash_setup:
    snakefile: "./zcash_setup.smk"
use rule * from zcash_setup

# Directories
call_dir = const.common_generated_dir / "07-zcash-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"
zcash_connector_account_file = const.near_account_dir / f"{NEC.ZCASH_CONNECTOR}.json"
zcash_account_file = const.near_account_dir / f"{NEC.ZCASH_TOKEN}.json"

# Binary files
zcash_connector_binary_file = const.near_binary_dir / f"{NEC.ZCASH_CONNECTOR}.wasm"
zcash_binary_file = const.near_binary_dir / f"{NEC.ZCASH_TOKEN}.wasm"

zcash_connector_file = const.near_deploy_results_dir / f"{NEC.ZCASH_CONNECTOR}.json"
zcash_file = const.near_deploy_results_dir / f"{NEC.ZCASH_TOKEN}.json"

report_file = call_dir / "zcash-transfer-report.txt"

rule sync_zcash_connector:
    message: "Sync BTC connector"
    input:
        zcash_connector_file = zcash_connector_file,
        init_account_file = near_init_account_file
    output: call_dir / "01_sync_zcash_connector"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        scripts_dir = const.common_scripts_dir,
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
        {params.scripts_dir}/call-near-contract.sh -c {params.zcash_connector} \
        -m sync_chain_signatures_root_public_key \
        -f {input.init_account_file} \
        -d "1 yoctoNEAR"\
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule get_zcash_user_deposit_address:
    message: "Get BTC user deposit address"
    input:
        step_1 = rules.sync_zcash_connector.output,
        zcash_connector_file = zcash_connector_file,
        user_account_file = user_account_file
    output: call_dir / "02_zcash_user_deposit_address"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file
    shell: """
    {params.mkdir} && \
         bridge-cli testnet get-bitcoin-address \
         --chain zcash \
         --zcash-connector {params.zcash_connector} \
         -r near:{params.user_account_id} \
         --amount 0\
         --near-signer {params.user_account_id} \
         --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_zcash_to_deposit_address:
    message: "Send TAZ to user deposit address on ZCash"
    input:
        step_2 = rules.get_zcash_user_deposit_address.output
    output: call_dir / "03_send_zcash_to_deposit_address"
    params:
        scripts_dir = const.common_scripts_dir,
        zcash_address = lambda wc, input: get_btc_address(input.step_2)
    shell: """
        {params.scripts_dir}/send_zcash.sh {params.zcash_address} > {output}
    """

rule wait_tx:
    message: "Wait for ZCash transaction"
    input:
        prev_step = call_dir / "03_send_zcash_to_deposit_address",
    output: call_dir / "03_1_wait_tx.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_tx_hash = lambda wc, input: get_zcash_tx_hash(input.prev_step),
    shell: """
    node {params.scripts_dir}/wait_btc.js zcash {params.btc_tx_hash} {output}
    """

rule fin_zcash_transfer_on_near:
    message: "Finalizing Zcash transfer on Near"
    input:
        prev_step = call_dir / "03_1_wait_tx.json",
        step_3 = rules.send_zcash_to_deposit_address.output,
        zcash_connector_file = zcash_connector_file,
        zcash_file = zcash_file,
        user_account_file = user_account_file
    output: call_dir / "04_fin_zcash_transfer_on_near"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        zcash_tx_hash = lambda wc, input: get_zcash_tx_hash(input.step_3),
    shell: """
    bridge-cli testnet  near-fin-transfer-btc \
        --chain zcash \
        -b {params.zcash_tx_hash} \
        -v 0 \
        -r {params.user_account_id} \
        --zcash-connector {params.zcash_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule init_zcash_transfer_to_zcash:
    message: "Init transfer from ZCash to Near"
    input:
        step_4 = rules.fin_zcash_transfer_on_near.output,
        zcash_connector_file = zcash_connector_file,
        zcash_file = zcash_file,
        user_account_file = user_account_file
    output: call_dir / "05_init_zcash_transfer_to_zcash"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        zcash_token = lambda wc, input: get_json_field(input.zcash_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    set -a
    source "$PWD/tools/.env"
    set +a
    
    bridge-cli testnet  init-near-to-bitcoin-transfer\
        --chain zcash \
        --target-btc-address $ZCASH_ACCOUNT_ID \
        --amount 3000 \
        --zcash-connector {params.zcash_connector} \
        --zcash {params.zcash_token} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule submit_transfer_to_btc_connector:
    message: "Sign BTC transfer on OmniBridge"
    input:
       step_5 = rules.init_zcash_transfer_to_zcash.output,
       zcash_connector_file = zcash_connector_file,
       user_account_file = user_account_file
    output: call_dir / "06_sign_btc_transfer"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_5),
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,

    shell: """
    bridge-cli testnet near-sign-btc-transaction \
        --near-tx-hash {params.near_tx_hash} \
        --user-account {params.user_account_id} \
        --chain zcash \
        --zcash-connector {params.zcash_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_btc_transfer:
    message: "Send ZCash transfer"
    input:
        step_6 = rules.submit_transfer_to_btc_connector.output,
        zcash_connector_file = zcash_connector_file,
        user_account_file = user_account_file,
    output: call_dir / "07_send_zcash_transfer"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_6),
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
    --chain zcash \
    --zcash-connector {params.zcash_connector} \
    --near-tx-hash {params.near_tx_hash} \
    --relayer {params.user_account_id} \
    --config {params.bridge_sdk_config_file} \
    > {output} \
    """

rule wait_final_tx:
    message: "Wait for Final ZCash transaction"
    input:
        prev_step = call_dir / "07_send_zcash_transfer",
    output: call_dir / "wait_final_tx.json"
    params:
        scripts_dir = const.common_scripts_dir,
        zcash_tx_hash = lambda wc, input: get_last_value(input.prev_step),
    shell: """
    node {params.scripts_dir}/wait_btc.js zcash {params.zcash_tx_hash} {output}
    """

rule verify_withdraw:
    message: "Verify withdraw"
    input:
        step_7 = rules.send_btc_transfer.output,
        prev_step = call_dir / "wait_final_tx.json",
        zcash_connector_file = zcash_connector_file,
        zcash_file = zcash_file,
        user_account_file = user_account_file
    output: call_dir / "08_verify_withdraw.json"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        zcash_tx_hash = lambda wc, input: get_last_value(input.step_7),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output),
    shell: """
        bridge-cli testnet btc-verify-withdraw \
        --chain zcash \
        -b {params.zcash_tx_hash} \
        --zcash-connector {params.zcash_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} && \
        {params.extract_tx}\
    """

rule verify_last_transfer:
    message: "Zcash Transfer. Verification"
    input:
        rules.verify_withdraw.output,
    output: report_file
    params:
        config_file = const.common_bridge_sdk_config_file,
        call_dir = call_dir
    shell: """
        yarn --cwd {const.common_tools_dir} --silent verify-near-transfer \
        --tx-dir {params.call_dir} \
        | tee {output}
    """

rule all:
    input:
        rules.verify_last_transfer.output,
    default_target: True
