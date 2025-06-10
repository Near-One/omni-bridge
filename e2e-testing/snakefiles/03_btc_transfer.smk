import pathlib
import const
import time
from const import (get_evm_deploy_results_dir, get_evm_account_dir,
                     EvmNetwork as EN, NearContract as NC, EvmContract as EC, NearTestAccount as NTA)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

# Directories
call_dir = const.common_generated_dir / "03-btc-transfer"

# Account files
btc_connector_account_file = const.near_account_dir / f"btc_connector.json"
nbtc_account_file = const.near_account_dir / f"nbtc.json"
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"

# Binary files
btc_connector_binary_file = const.near_binary_dir / "btc_connector.wasm"
nbtc_binary_file = const.near_binary_dir / "nbtc.wasm"

# NEAR contract deployment
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

rule get_btc_connector_binary_file:
    output: btc_connector_binary_file
    params:
        mkdir_cmd = get_mkdir_cmd(const.near_binary_dir)
    shell: """
    {params.mkdir_cmd} && \
    wget https://github.com/Near-Bridge-Lab/resources/raw/refs/heads/master/contracts/satoshi_bridge_release.wasm -O {output}
    """

rule get_nbtc_binary_file:
    output: nbtc_binary_file
    params:
        mkdir_cmd = get_mkdir_cmd(const.near_binary_dir)
    shell: """
    {params.mkdir_cmd} && \
    wget https://github.com/Near-Bridge-Lab/resources/raw/refs/heads/master/contracts/nbtc_release.wasm -O {output}
    """

rule near_generate_nbtc_init_args:
    message: "Generating nbtc init args"
    input:
        btc_connector_account_file = btc_connector_account_file,
        near_dao_account_file = near_dao_account_file
    output: const.common_generated_dir / "nbtc_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller = lambda wc, input: get_json_field(input.near_dao_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.btc_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"controller\": \"{params.controller}\", \"bridge_id\":\"{params.bridge_id}\"}}' > {output}
    """

rule near_generate_btc_connector_init_args:
    message: "Generating btc-connector init args"
    input:
        btc_connector_account_file = btc_connector_account_file,
        nbtc_account_file = nbtc_account_file,
    output: const.common_generated_dir / "btc_connector_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        nbtc_id = lambda wc, input: get_json_field(input.nbtc_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.btc_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.nbtc_id} send-near {params.bridge_id} '1 NEAR' network-config testnet sign-with-keychain send &&\
    echo '{{\"config\": {{\"chain_signatures_account_id\": \"v1.signer-prod.testnet\",\"nbtc_account_id\": \"{params.nbtc_id}\",\"btc_light_client_account_id\": \"btc-client.testnet\",\"confirmations_strategy\": {{\"100000000\": 6}},\"confirmations_delta\": 1,\"withdraw_bridge_fee\": {{\"fee_min\": \"1000\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"deposit_bridge_fee\": {{\"fee_min\": \"2000\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"min_deposit_amount\": \"5000\", \"min_withdraw_amount\": \"5000\", \"min_change_amount\": \"0\", \"max_change_amount\": \"100000000\",\"min_btc_gas_fee\": \"100\",\"max_btc_gas_fee\": \"80000\",\"max_withdrawal_input_number\": 10,\"max_change_number\": 10,\"max_active_utxo_management_input_number\": 10,\"max_active_utxo_management_output_number\": 10,\"active_management_lower_limit\": 0,\"active_management_upper_limit\": 1000,\"passive_management_lower_limit\": 0,\"passive_management_upper_limit\": 600,\"rbf_num_limit\": 99,\"max_btc_tx_pending_sec\": 86400}}}}' > {output}
    """

rule all:
    input:
        nbtc_file,
        btc_connector_file,
    default_target: True
