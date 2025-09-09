import const
import pathlib
from const import (NearContract as NC, NearTestAccount as NTA, EvmContract as EC, get_evm_account_dir)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

module btc_setup:
    snakefile: "./btc_setup.smk"
use rule * from btc_setup

# Directories
call_dir = const.common_generated_dir / "08-btc-evm-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

evm_account_file = pathlib.Path(get_evm_account_dir("sepolia")) / f"{EC.USER_ACCOUNT}.json"

rule get_btc_user_deposit_address:
    message: "Get BTC user deposit address"
    input:
        step_1 = rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file,
        evm_account = evm_account_file,
        omni_bridge_file = omni_bridge_file
    output: call_dir / "02_btc_user_deposit_address.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        token_locker_id = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        recipient_address = lambda wc, input: get_json_field(input.evm_account, "address"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file
    shell: """
    {params.mkdir} && \
         bridge-cli testnet get-bitcoin-address \
         --chain bitcoin-testnet \
         --btc-connector {params.btc_connector} \
         --amount 7300 \
         -r eth:{params.recipient_address} \
         --near-token-locker-id {params.token_locker_id} \
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
        btc_address = lambda wc, input: get_btc_address(input.step_2)
    shell: """
        node {params.scripts_dir}/send_btc.js {params.btc_address} 7500 > {output}
    """

rule add_omni_bridge_to_whitelist:
    message: "Add OmniBridge to whitelist for Post Action in Btc Connector"
    input:
        step_1 = rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        near_init_account_file = near_init_account_file,
        omni_bridge_file = omni_bridge_file
    output: call_dir / "add_omni_bridge_to_whitelist.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        scripts_dir = const.common_scripts_dir,
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        token_locker_id = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
        {params.mkdir} && \
        {params.scripts_dir}/call-near-contract.sh -c {params.btc_connector} \
           -m extend_post_action_receiver_id_white_list \
           -a '{{\"receiver_ids\": [\"{params.token_locker_id}\"]}}' \
           -f {input.near_init_account_file} \
           -d "1 yoctoNEAR"\
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

rule omni_bridge_storage_deposit_0:
    message: "Depositing storage for Omni Bridge on Omni Bridge"
    input:
        omni_bridge_contract_file = omni_bridge_file,
        user_account_file = user_account_file
    output:
        call_dir / "omni_bridge_storage_deposit_0.json"
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


rule fin_btc_transfer_on_near:
    message: "Finalizing BTC transfer on Near"
    input:
        omni_bridge_whitelist = rules.add_omni_bridge_to_whitelist.output,
        add_utxo_chain_connector = rules.add_utxo_chain_connector.output,
        omni_bridge_storage_deposit_0 = rules.omni_bridge_storage_deposit_0.output,
        step_3 = rules.send_btc_to_deposit_address.output,
        btc_connector_file = btc_connector_file,
        nbtc_file = nbtc_file,
        user_account_file = user_account_file,
        evm_account = evm_account_file,
        omni_bridge_file = omni_bridge_file
    output: call_dir / "04_fin_btc_transfer_on_near.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        token_locker_id = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_3),
        recipient_address = lambda wc, input: get_json_field(input.evm_account, "address"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
    bridge-cli testnet  near-fin-transfer-btc \
        --chain bitcoin-testnet \
        -b {params.btc_tx_hash} \
        -v 0 \
        -r eth:{params.recipient_address} \
        --amount 7300 \
        --btc-connector {params.btc_connector} \
        --near-token-locker-id {params.token_locker_id} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} && \
        {params.extract_tx}
    """

rule near_sign_transfer:
    message: "Transfer token from Bitcoin to Ethereum. Sign transfer on Near"
    input:
        near_init_transfer = rules.fin_btc_transfer_on_near.output,
        sender_account = user_account_file,
        bridge_contract = omni_bridge_file,
    output: call_dir / "05_sign-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        sender_account_id = lambda wc, input: get_json_field(input.sender_account, "account_id"),
        sender_private_key = lambda wc, input: get_json_field(input.sender_account, "private_key"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        init_transfer_tx_hash = lambda wc, input: get_json_field(input.near_init_transfer, "tx_hash"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
        {params.mkdir} && \
        NONCE=$(yarn --cwd {const.common_tools_dir} --silent get-near-transfer-nonce \
            --tx-hash {params.init_transfer_tx_hash} -r 7) && \
        bridge-cli testnet near-sign-transfer \
        --origin-chain Near \
        --origin-nonce $NONCE \
        --fee 0 \
        --native-fee 0 \
        --near-signer {params.sender_account_id} \
        --near-private-key {params.sender_private_key} \
        --near-token-locker-id {params.token_locker_id} \
        --config {params.config_file} > {output} && \
        {params.extract_tx}
    """

rule transfer_btc_to_evm_all:
    input: rules.near_sign_transfer.output
    message: "Transfer BTC to EVM pipeline completed"
    default_target: True
