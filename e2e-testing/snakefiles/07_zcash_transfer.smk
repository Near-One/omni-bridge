import pathlib
import const
import time
from const import (get_evm_deploy_results_dir, get_evm_account_dir,
                     EvmNetwork as EN, NearContract as NC, EvmContract as EC, NearTestAccount as NTA)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

# Directories
call_dir = const.common_generated_dir / "07-zcash-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"
zcash_connector_account_file = const.near_account_dir / f"zcash_connector.json"
zcash_account_file = const.near_account_dir / f"zcash.json"

# Binary files
zcash_connector_binary_file = const.near_binary_dir / "zcash_connector.wasm"
zcash_binary_file = const.near_binary_dir / "zcash.wasm"

zcash_file = const.near_deploy_results_dir / f"zcash.json"
zcash_connector_file = const.near_deploy_results_dir / f"zcash_connector.json"

rule near_generate_zcash_init_args:
    message: "Generating zcash init args"
    input:
        zcash_connector_account_file = zcash_connector_account_file,
        near_dao_account_file = near_dao_account_file
    output: const.common_generated_dir / "zcash_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller = lambda wc, input: get_json_field(input.near_dao_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"controller\": \"{params.controller}\", \"bridge_id\":\"{params.bridge_id}\"}}' > {output}
    """

rule near_generate_zcash_connector_init_args:
    message: "Generating btc-connector init args"
    input:
        zcash_connector_account_file = zcash_connector_account_file,
        zcash_account_file = zcash_account_file,
    output: const.common_generated_dir / "zcash_connector_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        zcash_id = lambda wc, input: get_json_field(input.zcash_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.zcash_id} send-near {params.bridge_id} '3 NEAR' network-config testnet sign-with-keychain send &&\
    echo '{{\"config\": {{\"chain\": \"ZcashTestnet\", \"chain_signatures_account_id\": \"v1.signer-prod.testnet\",\"nbtc_account_id\": \"{params.zcash_id}\",\"btc_light_client_account_id\": \"zcash-client.n-bridge.testnet\",\"confirmations_strategy\": {{\"100000000\": 6}},\"confirmations_delta\": 1,\"withdraw_bridge_fee\": {{\"fee_min\": \"400\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"deposit_bridge_fee\": {{\"fee_min\": \"200\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"min_deposit_amount\": \"500\", \"min_withdraw_amount\": \"500\", \"min_change_amount\": \"0\", \"max_change_amount\": \"100000000\",\"min_btc_gas_fee\": \"100\",\"max_btc_gas_fee\": \"80000\",\"max_withdrawal_input_number\": 10,\"max_change_number\": 10,\"max_active_utxo_management_input_number\": 10,\"max_active_utxo_management_output_number\": 10,\"active_management_lower_limit\": 0,\"active_management_upper_limit\": 1000,\"passive_management_lower_limit\": 0,\"passive_management_upper_limit\": 600,\"rbf_num_limit\": 99,\"max_btc_tx_pending_sec\": 86400, \"expiry_height_gap\": 100}}}}' > {output}
    """

rule sync_zcash_connector:
    message: "Sync BTC connector"
    input:
        zcash_connector_file = zcash_connector_file,
        init_account_file = near_init_account_file
    output: call_dir / "01_sync_zcash_connector.json"
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
    output: call_dir / "02_zcash_user_deposit_address.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file
    shell: """
    {params.mkdir} && \
         bridge-cli testnet get-bitcoin-address \
         --chain zcash \
         --btc-connector {params.zcash_connector} \
         -r {params.user_account_id} \
         --near-signer {params.user_account_id} \
         --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_zcash_to_deposit_address:
    message: "Send TAZ to user deposit address on ZCash"
    input:
        step_2 = rules.get_zcash_user_deposit_address.output,
    output: call_dir / "03_send_zcash_to_deposit_address.json"
    params:
        scripts_dir = const.common_scripts_dir,
        zcash_address = lambda wc, input: get_btc_address(input.step_2),
    shell: """
    node {params.scripts_dir}/send_btc.js {params.zcash_address} 7500 > {output}
    """

rule fin_zcash_transfer_on_near:
    message: "Finalizing Zcash transfer on Near"
    input:
        step_3 = rules.send_zcash_to_deposit_address.output,
        zcash_connector_file = zcash_connector_file,
        zcash_file = zcash_file,
        user_account_file = user_account_file
    output: call_dir / "04_fin_zcash_transfer_on_near.json"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        zcash_tx_hash = lambda wc, input: get_last_value(input.step_3),
    shell: """
    bridge-cli testnet  near-fin-transfer-btc \
        -b {params.zcash_tx_hash} \
        -v 0 \
        -r {params.user_account_id} \
        --btc-connector {params.zcash_connector} \
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
        user_account_file = user_account_file
    output: call_dir / "05_init_zcash_transfer_to_zcash.json"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet  init-near-to-bitcoin-transfer\
        --target-btc-address utest1rj797prep5mnq9ffd2zddlc94a4jm4z9kekluh87x7zvhq2sy3a4vence62nt0gzvjxyg06xtgmzrnrx6a8yv63gfa97j5rt55fkmlzjhpcgj4w85vgz5uphsp065g2kj9dk24f0kyl5f7jf36sdtt7ley6vucxftpekvsceg8upfeluev5308d0l3ycsnfs43uc8x3ggqc27danjtp \
        --amount 5000 \
        --btc-connector {params.zcash_connector} \
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
    output: call_dir / "06_sign_btc_transfer.json"
    params:
        zcash_connector = lambda wc, input: get_json_field(input.zcash_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,

    shell: """
    bridge-cli testnet near-sign-btc-transaction \
        --btc-pending-id 04238d52945a710acd99823a0d12791266d3558a38d5914b32eed7c97e15c257 \
        --btc-connector {params.zcash_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_btc_transfer:
    message: "Send ZCash transfer"
    input:
        step_6 = rules.submit_transfer_to_btc_connector.output,
        user_account_file = user_account_file,
    output: call_dir / "07_send_zcash_transfer.json"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_6),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
    --near-tx-hash {params.near_tx_hash} \
    --satoshi-relayer {params.user_account_id} \
    --config {params.bridge_sdk_config_file} \
    > {output} \
    """

rule all:
    input:
        rules.send_btc_transfer.output,
    default_target: True
