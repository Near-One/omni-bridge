import const
import pathlib
from const import (NearContract as NC, NearTestAccount as NTA, EvmContract as EC, get_evm_account_dir)
from utils import progress_wait, get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

module btc_setup:
    snakefile: "./btc_setup.smk"
use rule * from btc_setup

module evm:
    snakefile: "./evm.smk"
use rule * from evm
# Directories
call_dir = const.common_generated_dir / "08-btc-evm-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

evm_deploy_results_dir = pathlib.Path(const.get_evm_deploy_results_dir("sepolia"))
evm_account_file = pathlib.Path(get_evm_account_dir("sepolia")) / f"{EC.USER_ACCOUNT}.json"
evm_bridge_contract_file = evm_deploy_results_dir / f"{EC.OMNI_BRIDGE}.json"
evm_prover_setup_file = const.near_deploy_results_dir / "sepolia-evm-prover-setup-call.json"

rule get_btc_user_deposit_address:
    message: "Get BTC user deposit address"
    input:
        step_1 = rules.sync_btc_connector.output,
        btc_connector_file = btc_connector_file,
        user_account_file = user_account_file,
        evm_account = evm_account_file,
        omni_bridge_file = omni_bridge_file
    output: call_dir / "01_btc_user_deposit_address"
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
    output: call_dir / "02_send_btc_to_deposit_address"
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
    output: call_dir / "03_add_omni_bridge_to_whitelist.json"
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
    output: call_dir / "04_add_utxo_chain_connector.json"
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

rule omni_bridge_storage_deposit_for_omni_bridge:
    message: "Depositing storage for Omni Bridge on Omni Bridge"
    input:
        omni_bridge_contract_file = omni_bridge_file,
        user_account_file = user_account_file
    output:
        call_dir / "05_omni_bridge_storage_deposit_for_omni_bridge.json"
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
        omni_bridge_storage_deposit_0 = rules.omni_bridge_storage_deposit_for_omni_bridge.output,
        step_3 = rules.send_btc_to_deposit_address.output,
        btc_connector_file = btc_connector_file,
        nbtc_file = nbtc_file,
        user_account_file = user_account_file,
        evm_account = evm_account_file,
        omni_bridge_file = omni_bridge_file
    output: call_dir / "06_fin_btc_transfer_on_near.json"
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
    bridge-cli testnet near-fin-transfer-btc \
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

rule add_evm_factory_to_locker:
    message: "Adding Ethereum factory to Omni Bridge"
    input:
        bridge_contract = omni_bridge_file,
        init_account = near_init_account_file,
        evm_bridge = evm_bridge_contract_file
    output: call_dir / "07_add_evm_factory_to_locker.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        factory_address = lambda wc, input: get_json_field(input.evm_bridge, "bridgeAddress"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
        {params.mkdir} && \
        {const.common_scripts_dir}/call-near-contract.sh -c {params.token_locker_id} \
            -m add_factory \
            -a '{{\"address\": \"{params.factory_address}\"}}' \
            -f {input.init_account} \
            -n testnet 2>&1 | tee {output} && \
        {params.extract_tx}
        """

rule near_log_metadata_call:
    message: "Bridge NEAR Token to EVM. Logging token metadata"
    input:
        sender_account = user_account_file,
        bridge_contract = omni_bridge_file,
        test_token = nbtc_file,
    output: call_dir / "08_log_metadata.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        token_id = lambda wc, input: get_json_field(input.test_token, "contract_id"),
        sender_account_id = lambda wc, input: get_json_field(input.sender_account, "account_id"),
        sender_private_key = lambda wc, input: get_json_field(input.sender_account, "private_key"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
        {params.mkdir} && \
        bridge-cli testnet log-metadata \
            --token near:{params.token_id} \
            --near-signer {params.sender_account_id} \
            --near-private-key {params.sender_private_key} \
            --near-token-locker-id {params.token_locker_id} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
    """

rule evm_deploy_token:
    message: "Deploying BTC token on Ethereum"
    input:
        log_metadata = rules.near_log_metadata_call.output,
        evm_bridge = evm_bridge_contract_file,
    output: call_dir / "09_evm_deploy_token.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(10),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        near_chain_str = const.Chain.NEAR,
        log_metadata_tx_hash = lambda wc, input: get_json_field(input.log_metadata, "tx_hash"),
        evm_bridge_token_factory_address = lambda wc, input: get_json_field(input.evm_bridge, "bridgeAddress"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        bridge-cli testnet deploy-token \
            --chain {params.evm_chain_str} \
            --source-chain {params.near_chain_str} \
            --tx-hash {params.log_metadata_tx_hash} \
            --eth-bridge-token-factory-address {params.evm_bridge_token_factory_address} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
     """

rule near_bind_token:
    message: "Binding BTC token on NEAR"
    input:
        rules.add_evm_factory_to_locker.output,
        evm_prover_setup_file,
        evm_deploy_token = rules.evm_deploy_token.output,
        relayer_account = user_account_file,
        bridge_contract = omni_bridge_file
    output: call_dir / "10_near_bind_token.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(1300),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        evm_deploy_token_tx_hash = lambda wc, input: get_json_field(input.evm_deploy_token, "tx_hash"),
        relayer_account_id = lambda wc, input: get_json_field(input.relayer_account, "account_id"),
        relayer_private_key = lambda wc, input: get_json_field(input.relayer_account, "private_key"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        bridge-cli testnet bind-token \
            --chain {params.evm_chain_str} \
            --tx-hash {params.evm_deploy_token_tx_hash} \
            --near-signer {params.relayer_account_id} \
            --near-private-key {params.relayer_private_key} \
            --near-token-locker-id {params.token_locker_id} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
    """

rule near_sign_transfer:
    message: "Transfer token from Bitcoin to Ethereum. Sign transfer on Near"
    input:
        near_init_transfer = rules.fin_btc_transfer_on_near.output,
        near_bind_token = rules.near_bind_token.output,
        sender_account = user_account_file,
        bridge_contract = omni_bridge_file,
    output: call_dir / "11_sign-transfer.json"
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

rule evm_fin_transfer:
    message: "Fin transfer on EVM"
    input:
        near_sign_transfer = rules.near_sign_transfer.output,
        evm_bridge = evm_bridge_contract_file,
    output: call_dir / "12_eth_fin-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        sign_transfer_tx_hash = lambda wc, input: get_json_field(input.near_sign_transfer, "tx_hash"),
        evm_bridge_address = lambda wc, input: get_json_field(input.evm_bridge, "bridgeAddress"),
        progress_wait_cmd = progress_wait(20),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
            {params.mkdir} && \
            {params.progress_wait_cmd} \
            bridge-cli testnet evm-fin-transfer \
                --chain {params.evm_chain_str} \
                --tx-hash {params.sign_transfer_tx_hash} \
                --eth-bridge-token-factory-address {params.evm_bridge_address} \
                --config {params.config_file} > {output} && \
            {params.extract_tx}
    """

rule verify_transfer_near_to_evm:
    message: "Verification"
    input:
        rules.evm_fin_transfer.output,
        test_token = nbtc_file,
        bridge_contract = omni_bridge_file,
        evm_account = evm_account_file,
    output: call_dir / "report"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        near_token_id = lambda wc, input: get_json_field(input.test_token, "contract_id"),
        progress_wait_cmd = progress_wait(5),
        recipient_address = lambda wc, input: get_json_field(input.evm_account, "address"),
    shell: """
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        yarn --cwd {const.common_tools_dir} --silent verify-transfer-near-to-evm \
            --tx-dir {call_dir} \
            --receiver {params.recipient_address} \
            --near-token {params.near_token_id} \
            --chain-kind {params.evm_chain_str} \
            --near-locker {params.token_locker_id} \
            | tee {output}
        """

rule transfer_btc_to_evm_all:
    input: rules.verify_transfer_near_to_evm.output
    message: "Transfer BTC to EVM pipeline completed"
    default_target: True
