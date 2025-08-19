import pathlib
import const
import time
from const import NearContract as NC, NearTestAccount as NTA, EvmContract as EC, get_evm_account_dir
from utils import progress_wait, get_json_field, extract_tx_hash, get_mkdir_cmd

module transfer_near_to_evm:
    snakefile: "./02_transfer_near_to_evm.smk"
use rule * from transfer_near_to_evm

evm_deploy_results_dir = pathlib.Path(const.get_evm_deploy_results_dir("{network}"))
evm_bridge_contract_file = evm_deploy_results_dir / f"{EC.OMNI_BRIDGE}.json"
evm_account_file = pathlib.Path(get_evm_account_dir("{network}")) / f"{EC.USER_ACCOUNT}.json"

near_token_owner_credentials_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_sender_account_file = const.near_account_dir / f"{NTA.SENDER_ACCOUNT}.json"
near_relayer_account_file = const.near_account_dir / f"{NTA.RELAYER_ACCOUNT}.json"
near_bridge_contract_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
near_test_token_file = const.near_deploy_results_dir / f"{NC.MOCK_TOKEN}.json"

call_dir = const.common_generated_dir / "03-fast-transfer-{network}-to-near"
report_file = call_dir / "verify-fast-transfer-report.txt"

fast_transfer_amount = 1000

# Main pipeline rule
# TODO: Replace ETH_SEPOLIA with all EVM networks when the `evm_deploy_token` rule doesn't crash on Base and Arbitrum (the issue is not in the pipeline)
rule fast_transfer_evm_to_near_all:
    input:
        expand(report_file, 
               network=list([const.EvmNetwork.ETH_SEPOLIA]))
            #    network=list(const.EvmNetwork))
    message: "Transfer Near to EVM pipeline completed"
    default_target: True

rule near_fund_relayer:
    message: "Transfer token from Near to {wildcards.network}. Step 0: Fund relayer account with test tokens"
    input:
        rules.verify_transfer_near_to_evm.output,
        near_relayer_account = near_relayer_account_file,
        near_test_token = near_test_token_file,
        near_owner_account = near_token_owner_credentials_file
    output: call_dir / "00_fund-sender.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        token_id = lambda wc, input: get_json_field(input.near_test_token, "contract_id"),
        relayer_account_id = lambda wc, input: get_json_field(input.near_relayer_account, "account_id"),
        fast_transfer_amount = fast_transfer_amount * 1000000,
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.token_id} \
        -m storage_deposit \
        -a '{{\"account_id\": \"{params.relayer_account_id}\"}}' \
        -f {input.near_owner_account} \
        -d 0.00235NEAR \
        -n testnet 2>&1 | tee {output} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.token_id} \
        -m ft_transfer \
        -a '{{\"receiver_id\": \"{params.relayer_account_id}\", \"amount\":\"{params.fast_transfer_amount}\"}}' \
        -f {input.near_owner_account} \
        -d 1YOCTONEAR \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule evm_init_transfer:
    message: "Fast transfer from {wildcards.network} to Near. Step 1: Init transfer on EVM"
    input:
        rules.near_fund_relayer.output,
        evm_bridge = evm_bridge_contract_file,
        near_receiver_account = near_sender_account_file,
        near_test_token = near_test_token_file,
        bridge_contract = near_bridge_contract_file,
    output: call_dir / "01_init-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        evm_bridge_address = lambda wc, input: get_json_field(input.evm_bridge, "bridgeAddress"),
        near_receiver_account_id = lambda wc, input: get_json_field(input.near_receiver_account, "account_id"),
        token_id = lambda wc, input: get_json_field(input.near_test_token, "contract_id"),
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
            --amount {fast_transfer_amount} \
            --recipient near:{params.near_receiver_account_id} \
            --fee 300 \
            --native-fee 0 \
            --eth-bridge-token-factory-address {params.evm_bridge_address} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
        """

rule near_fast_transfer:
    message: "Fast transfer from {wildcards.network} to Near. Step 2: Fast transfer on Near"
    input:
        init_transfer = rules.evm_init_transfer.output,
        relayer_account = near_relayer_account_file,
        bridge_contract = near_bridge_contract_file,
    output: call_dir / "02_fast-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(10),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        init_transfer_tx_hash = lambda wc, input: get_json_field(input.init_transfer, "tx_hash"),
        relayer_account_id = lambda wc, input: get_json_field(input.relayer_account, "account_id"),
        relayer_private_key = lambda wc, input: get_json_field(input.relayer_account, "private_key"),
        bridge_contract_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        bridge-cli testnet near-fast-fin-transfer \
            --chain {params.evm_chain_str} \
            --tx-hash {params.init_transfer_tx_hash} \
            --near-signer {params.relayer_account_id} \
            --near-private-key {params.relayer_private_key} \
            --near-token-locker-id {params.bridge_contract_id} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
        """

rule near_fin_transfer:
    message: "Fast transfer from {wildcards.network} to Near. Step 3: Fast transfer on Near"
    input:
        rules.near_fast_transfer.output,
        init_transfer = rules.evm_init_transfer.output,
        relayer_account = near_relayer_account_file,
        near_test_token = near_test_token_file,
        bridge_contract = near_bridge_contract_file,
    output: call_dir / "03_fin-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(1),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        init_transfer_tx_hash = lambda wc, input: get_json_field(input.init_transfer, "tx_hash"),
        relayer_account_id = lambda wc, input: get_json_field(input.relayer_account, "account_id"),
        relayer_private_key = lambda wc, input: get_json_field(input.relayer_account, "private_key"),
        bridge_contract_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        token_id = lambda wc, input: get_json_field(input.near_test_token, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell:"""
        {params.mkdir} && \
        {params.progress_wait_cmd} \
        bridge-cli testnet near-fin-transfer \
            --chain {params.evm_chain_str} \
            --tx-hash {params.init_transfer_tx_hash} \
            --near-signer {params.relayer_account_id} \
            --near-private-key {params.relayer_private_key} \
            --near-token-locker-id {params.bridge_contract_id} \
            --config {params.config_file} > {output} && \
        {params.extract_tx}
        """

# 3. Verify transfer
rule verify_fast_transfer:
    message: "Fast transfer from {wildcards.network} to Near. Verification"
    input:
        rules.near_fin_transfer.output,
    output: report_file
    params:
        config_file = const.common_bridge_sdk_config_file,
        call_dir = lambda wildcards: str(call_dir).format(network=wildcards.network),
        mkdir = get_mkdir_cmd(call_dir),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
    {params.mkdir} && \
    yarn --cwd {const.common_tools_dir} --silent verify-fast-transfer-evm-to-near \
        --tx-dir {params.call_dir} \
        | tee {output}
    """