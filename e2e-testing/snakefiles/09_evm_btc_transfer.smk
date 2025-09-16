import const
import pathlib
from const import (NearContract as NC, NearTestAccount as NTA, EvmContract as EC, get_evm_account_dir)
from utils import progress_wait, get_mkdir_cmd, get_json_field, extract_tx_hash, get_tx_hash, get_last_value

module near:
    snakefile: "./near.smk"
use rule * from near

module evm:
    snakefile: "./evm.smk"
use rule * from evm

module btc_evm_transfer:
    snakefile: "./08_btc_evm_transfer.smk"
use rule * from btc_evm_transfer

# Directories
call_dir = const.common_generated_dir / "09-evm-btc-transfer"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"
near_relayer_account_file = const.near_account_dir / f"{NTA.RELAYER_ACCOUNT}.json"

evm_deploy_results_dir = pathlib.Path(const.get_evm_deploy_results_dir("sepolia"))
evm_account_file = pathlib.Path(get_evm_account_dir("sepolia")) / f"{EC.USER_ACCOUNT}.json"
evm_bridge_contract_file = evm_deploy_results_dir / f"{EC.OMNI_BRIDGE}.json"
evm_prover_setup_file = const.near_deploy_results_dir / "sepolia-evm-prover-setup-call.json"

rule evm_init_transfer:
    message: "Init transfer on EVM"
    input:
        rules.evm_fin_transfer.output,
        evm_bridge = evm_bridge_contract_file,
        nbtc_token = nbtc_file,
        bridge_contract = omni_bridge_file,
    output: call_dir / "01_init-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        evm_bridge_address = lambda wc, input: get_json_field(input.evm_bridge, "bridgeAddress"),
        token_id = lambda wc, input: get_json_field(input.nbtc_token, "contract_id"),
        bridge_contract_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
        {params.mkdir} && \
        TOKEN=$(yarn --cwd {const.common_tools_dir} --silent get-evm-token-address \
            --near-token {params.token_id} \
            --chain-kind {params.evm_chain_str} \
            --near-locker {params.bridge_contract_id}) && \
        bridge-cli testnet evm-init-transfer \
            --chain {params.evm_chain_str} \
            --token $TOKEN \
            --amount 5000 \
            --recipient btc:tb1q4vvl8ykwprwv9dw3y5nrnpk7f2jech7atz45v5 \
            --fee 300 \
            --native-fee 0 \
            --eth-bridge-token-factory-address {params.evm_bridge_address} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
    """

rule near_fin_transfer:
    message: "Fin transfer on Near"
    input:
        init_transfer = rules.evm_init_transfer.output,
        relayer_account = near_relayer_account_file,
        nbtc_token = nbtc_file,
        bridge_contract = omni_bridge_file,
    output: call_dir / "02_fin-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(1200),
        evm_chain_str = lambda wc: const.Chain.from_evm_network("sepolia"),
        init_transfer_tx_hash = lambda wc, input: get_json_field(input.init_transfer, "tx_hash"),
        relayer_account_id = lambda wc, input: get_json_field(input.relayer_account, "account_id"),
        relayer_private_key = lambda wc, input: get_json_field(input.relayer_account, "private_key"),
        bridge_contract_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        token_id = lambda wc, input: get_json_field(input.nbtc_token, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        bridge-cli testnet near-fin-transfer \
            --chain {params.evm_chain_str} \
            --destination-chain btc \
            --tx-hash {params.init_transfer_tx_hash} \
            --near-signer {params.relayer_account_id} \
            --near-private-key {params.relayer_private_key} \
            --near-token-locker-id {params.bridge_contract_id} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
    """

rule submit_transfer_to_btc_connector:
    message: "Sign BTC transfer on OmniBridge"
    input:
       step_2 = rules.near_fin_transfer.output,
       btc_connector_file = btc_connector_file,
       omni_bridge_file = omni_bridge_file,
       user_account_file = user_account_file,
       relayer_account = near_relayer_account_file,
    output: call_dir / "03_sign_btc_transfer"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        relayer_account_id = lambda wc, input: get_json_field(input.relayer_account, "account_id"),
        omni_bridge_account = lambda wc, input: get_json_field(input.omni_bridge_file, "contract_id"),
        near_tx_hash = lambda wc, input: get_json_field(input.step_2, "tx_hash"),

    shell: """
    bridge-cli testnet near-sign-btc-transfer \
        --chain btc \
        -n {params.near_tx_hash} \
        -s {params.omni_bridge_account} \
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
        step_3 = rules.submit_transfer_to_btc_connector.output,
        btc_connector_file = btc_connector_file,
        omni_bridge_file = omni_bridge_file,
        user_account_file = user_account_file
    output: call_dir / "04_sign_btc_connector_transfer"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_3)
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
        step_4 = rules.sign_btc_connector_transfer.output,
        user_account_file = user_account_file,
    output: call_dir / "05_send_btc_transfer"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_4),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
         --chain btc \
         --near-tx-hash {params.near_tx_hash} \
         --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule verify_withdraw:
    message: "Verify withdraw"
    input:
        step_5 = rules.send_btc_transfer.output,
        btc_connector_file = btc_connector_file,
        nbtc_file = nbtc_file,
        user_account_file = user_account_file
    output: call_dir / "06_verify_withdraw.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_5),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output),
    shell: """
        bridge-cli testnet btc-verify-withdraw \
        --chain btc \
        -b {params.btc_tx_hash} \
        --btc-connector {params.btc_connector} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} && \
        {params.extract_tx}\
    """

rule verify_last_transfer:
    message: "Verification"
    input:
        rules.verify_withdraw.output,
    output: call_dir / "report"
    params:
        config_file = const.common_bridge_sdk_config_file,
        call_dir = call_dir
    shell: """
        yarn --cwd {const.common_tools_dir} --silent verify-near-transfer \
        --tx-dir {params.call_dir} \
        | tee {output}
    """

rule transfer_evm_to_btc_all:
    input: rules.verify_last_transfer.output
    message: "Transfer EVM to BTC pipeline completed"
    default_target: True
