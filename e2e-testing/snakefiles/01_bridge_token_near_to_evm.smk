import pathlib
import const
import time
from const import NearContract as NC, NearTestAccount as NTA, EvmContract as EC
from utils import progress_wait, get_json_field, extract_tx_hash, get_mkdir_cmd

module evm:
    snakefile: "./evm.smk"
use rule * from evm as evm_*

call_dir = const.common_generated_dir / "01-bridge-token-near-to-evm"
evm_deploy_results_dir = pathlib.Path(const.get_evm_deploy_results_dir("{network}"))

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_sender_account_file = const.near_account_dir / f"{NTA.SENDER_ACCOUNT}.json"
near_relayer_account_file = const.near_account_dir / f"{NTA.RELAYER_ACCOUNT}.json"

# Contract files
near_bridge_contract_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
near_test_token_file = const.near_deploy_results_dir / f"{NC.MOCK_TOKEN}.json"
near_token_deployer_file = const.near_deploy_results_dir / f"{NC.TOKEN_DEPLOYER}.json"

evm_bridge_contract_file = evm_deploy_results_dir / f"{EC.OMNI_BRIDGE}.json"

# Call files
add_deployer_file = call_dir / "00-1_add-deployer-to-locker-call.json"
add_evm_factory_file = call_dir / "00-2_{network}-add-factory-to-locker-call.json"
log_metadata_file = call_dir / "01_omni-log-metadata-call.json"
evm_deploy_token_file = call_dir / "02_{network}-deploy-token-call.json"
near_bind_token_file = call_dir / "03_{network}-near-bind-token-call.json"

evm_prover_setup_file = const.near_deploy_results_dir / "{network}-evm-prover-setup-call.json"

# Main pipeline rule
# TODO: Replace ETH_SEPOLIA with all EVM networks when the `evm_deploy_token` rule doesn't crash on Base and Arbitrum (the issue is not in the pipeline)
rule all:
    input:
        expand(call_dir / "{network}-verify-bridge-token-report.txt", 
               network=list([const.EvmNetwork.ETH_SEPOLIA]))
            #    network=list(const.EvmNetwork))
    message: "Bridge NEAR Token to Ethereum pipeline completed"
    default_target: True


# Step 0: Add EVM factory to locker
rule add_evm_factory_to_locker:
    message: "Bridge NEAR Token to {wildcards.network}. Step 0: Adding {wildcards.network} factory to locker"
    input:
        bridge_contract = near_bridge_contract_file,
        init_account = near_init_account_file,
        evm_bridge = evm_bridge_contract_file
    output: add_evm_factory_file
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

# Step 1: Log metadata
rule near_log_metadata_call:
    message: "Bridge NEAR Token to EVM. Step 1: Logging token metadata"
    input:
        sender_account = near_sender_account_file,
        bridge_contract = near_bridge_contract_file,
        test_token = near_test_token_file,
    output: log_metadata_file
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

# Step 2: Deploy token on EVM
rule evm_deploy_token:
    message: "Bridge NEAR Token to {wildcards.network}. Step 2: Deploying token on {wildcards.network}"
    input:
        log_metadata = rules.near_log_metadata_call.output,
        evm_bridge = evm_bridge_contract_file,
    output: evm_deploy_token_file
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(10),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
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

# Step 3: Bind token on NEAR
rule near_bind_token:
    message: "Bridge NEAR Token to {wildcards.network}. Step 3: Binding token on NEAR"
    input:
        rules.add_evm_factory_to_locker.output,
        evm_prover_setup_file,
        evm_deploy_token = rules.evm_deploy_token.output,
        relayer_account = near_relayer_account_file,
        bridge_contract = near_bridge_contract_file
    output: near_bind_token_file
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        progress_wait_cmd = progress_wait(1300),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
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

# Step 4: Verify the correctness of the token bridging
rule verify_bridge_token_near_to_evm:
    message: "Bridge NEAR Token to {wildcards.network}. Verification"
    input:
        near_bind_token = rules.near_bind_token.output,
        tools_compile = const.common_tools_compile_stamp,
        test_token = near_test_token_file,
        bridge_contract = near_bridge_contract_file,
        evm_deploy_token = rules.evm_deploy_token.output
    output: call_dir / "{network}-verify-bridge-token-report.txt"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        evm_deploy_token_tx_hash = lambda wc, input: get_json_field(input.evm_deploy_token, "tx_hash"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        near_token_id = lambda wc, input: get_json_field(input.test_token, "contract_id"),
    shell: """
    {params.mkdir} && \
    yarn --cwd {const.common_tools_dir} --silent verify-bridge-token-near-to-evm \
        --tx-dir {call_dir} \
        --near-token {params.near_token_id} \
        --chain-kind {params.evm_chain_str} \
        --near-locker {params.token_locker_id} \
        --token-tx {params.evm_deploy_token_tx_hash} | tee {output}
    """
