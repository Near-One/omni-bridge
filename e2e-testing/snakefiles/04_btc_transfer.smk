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
call_dir = const.common_generated_dir / "04-btc-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"
btc_connector_account_file = const.near_account_dir / f"btc_connector.json"
nbtc_account_file = const.near_account_dir / f"nbtc.json"

# Binary files
btc_connector_binary_file = const.near_binary_dir / "btc_connector.wasm"
nbtc_binary_file = const.near_binary_dir / "nbtc.wasm"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
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
    echo '{{\"config\": {{\"chain_signatures_account_id\": \"v1.signer-prod.testnet\",\"nbtc_account_id\": \"{params.nbtc_id}\",\"btc_light_client_account_id\": \"btc-client-v4.testnet\",\"confirmations_strategy\": {{\"100000000\": 6}},\"confirmations_delta\": 1,\"withdraw_bridge_fee\": {{\"fee_min\": \"400\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"deposit_bridge_fee\": {{\"fee_min\": \"200\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"min_deposit_amount\": \"500\", \"min_withdraw_amount\": \"500\", \"min_change_amount\": \"0\", \"max_change_amount\": \"100000000\",\"min_btc_gas_fee\": \"100\",\"max_btc_gas_fee\": \"80000\",\"max_withdrawal_input_number\": 10,\"max_change_number\": 10,\"max_active_utxo_management_input_number\": 10,\"max_active_utxo_management_output_number\": 10,\"active_management_lower_limit\": 0,\"active_management_upper_limit\": 1000,\"passive_management_lower_limit\": 0,\"passive_management_upper_limit\": 600,\"rbf_num_limit\": 99,\"max_btc_tx_pending_sec\": 86400}}}}' > {output}
    """

rule sync_btc_connector:
    message: "Sync BTC connector"
    input:
        btc_connector_file = btc_connector_file,
        init_account_file = near_init_account_file
    output: call_dir / "01_sync_btc_connector.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        scripts_dir = const.common_scripts_dir,
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
        {params.scripts_dir}/call-near-contract.sh -c {params.btc_connector} \
        -m sync_chain_signatures_root_public_key \
        -f {input.init_account_file} \
        -d "1 yoctoNEAR"\
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule get_btc_user_deposit_address:
    message: "Get BTC user deposit address"
    input:
        step_1 = rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file
    output: call_dir / "02_btc_user_deposit_address.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file
    shell: """
    {params.mkdir} && \
         bridge-cli testnet get-bitcoin-address \
         --chain bitcoin-testnet \
         --btc-connector {params.btc_connector} \
         -r {params.user_account_id} \
         --near-signer {params.user_account_id} \
         --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule send_btc_to_deposit_address:
    message: "Send BTC to user deposit address on Bitcoin"
    input:
        step_2 = rules.get_btc_user_deposit_address.output,
    output: call_dir / "03_send_btc_to_deposit_address.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_address = lambda wc, input: get_btc_address(input.step_2),
    shell: """
    node {params.scripts_dir}/send_btc.js {params.btc_address} 7500 > {output}
    """

rule fin_btc_transfer_on_near:
    message: "Finalizing BTC transfer on Near"
    input:
        step_3 = rules.send_btc_to_deposit_address.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file
    output: call_dir / "04_fin_btc_transfer_on_near.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_3),
    shell: """
    bridge-cli testnet  near-fin-transfer-btc \
        --chain bitcoin-testnet \
        -b {params.btc_tx_hash} \
        -v 0 \
        -r {params.user_account_id} \
        --btc-connector {params.btc_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule omni_bridge_storage_deposit:
    message: "Depositing storage for User on Omni Bridge"
    input:
        step_4 = rules.fin_btc_transfer_on_near.output,
        omni_bridge_contract_file = omni_bridge_file,
        user_account_file = user_account_file
    output:
        call_dir / "06_omni_bridge_storage_deposit.json"
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_address} \
        -m storage_deposit \
        -a '{{\"account_id\": \"{params.user_account_id}\"}}' \
        -d "1 NEAR" \
        -f {input.user_account_file} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule add_utxo_chain_connector:
    message: "Add BTC connector to OmniBridge on Near"
    input:
        omni_bridge_file = omni_bridge_file,
        btc_connector_file = btc_connector_file,
        init_account_file = near_init_account_file,
        nbtc_file = nbtc_file,
    output: call_dir / "add_utxo_chain_connector.json"
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        btc_connector_account = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        nbtc_account = lambda wc, input: get_json_field(input.nbtc_file, "contract_id"),
    shell: """
        {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_account} \
            -m add_utxo_chain_connector \
            -a '{{\"chain_kind\": \"Btc\", \"utxo_chain_connector_id\": \"{params.btc_connector_account}\", \"utxo_chain_token_id\": \"{params.nbtc_account}\", \"decimals\": 8}}' \
            -f {input.init_account_file}  \
            -d "1 NEAR" \
            -n testnet 2>&1 | tee {output} && \
            TX_HASH=$(grep -o 'Transaction ID: [^ ]*' {output} | cut -d' ' -f3) && \
            echo '{{\"tx_hash\": \"'$TX_HASH'\"}}' > {output}
    """

rule ft_transfer_btc_to_omni_bridge:
    message: "Init BTC transfer to OmniBridge on Near"
    input:
        add_utxo_chain = rules.add_utxo_chain_connector.output,
        step_6 = rules.omni_bridge_storage_deposit.output,
        nbtc_file = nbtc_file,
        omni_bridge_file = omni_bridge_file,
        user_account_file = user_account_file,
    output: call_dir / "07_ft_transfer_btc_to_omni_bridge.json"
    params:
        scripts_dir = const.common_scripts_dir,
        nbtc_account = lambda wc, input: get_json_field(input.nbtc_file, "contract_id"),
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
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

rule submit_transfer_to_btc_connector:
    message: "Sign BTC transfer on OmniBridge"
    input:
       step_7 = rules.ft_transfer_btc_to_omni_bridge.output,
       btc_connector_file = btc_connector_file,
       omni_bridge_file = omni_bridge_file,
       user_account_file = user_account_file
    output: call_dir / "08_sign_btc_transfer.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        near_tx_hash = lambda wc, input: get_json_field(input.step_7, "tx_hash"),

    shell: """
    bridge-cli testnet omni-bridge-sign-btc-transfer \
        --chain bitcoin-testnet \
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
    message: "Sign BTC transfer on BtcConnectro"
    input:
        add_utxo_chain = rules.add_utxo_chain_connector.output,
        step_8 = rules.submit_transfer_to_btc_connector.output,
        btc_connector_file = btc_connector_file,
        omni_bridge_file = omni_bridge_file,
        user_account_file = user_account_file
    output: call_dir / "09_sign_btc_connector_transfer.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_8),

    shell: """
    bridge-cli testnet near-sign-btc-transaction \
        --chain bitcoin-testnet \
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
        step_9 = rules.sign_btc_connector_transfer.output,
        user_account_file = user_account_file,
    output: call_dir / "10_send_btc_transfer.json"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_9),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
    --chain bitcoin-testnet \
    --near-tx-hash {params.near_tx_hash} \
    --config {params.bridge_sdk_config_file} \
    > {output} \
    """

rule all:
    input:
        rules.send_btc_transfer.output,
    default_target: True
