import pathlib
import const
import time
from const import NearContract as NC, NearTestAccount as NTA, EvmContract as EC, get_evm_account_dir
from utils import progress_wait, get_json_field, extract_tx_hash, get_mkdir_cmd

module bridge_token_near_to_evm:
    snakefile: "./01_bridge_token_near_to_evm.smk"
use rule * from bridge_token_near_to_evm

evm_deploy_results_dir = pathlib.Path(const.get_evm_deploy_results_dir("{network}"))
evm_bridge_contract_file = evm_deploy_results_dir / f"{EC.OMNI_BRIDGE}.json"
evm_account_file = pathlib.Path(get_evm_account_dir("{network}")) / f"{EC.USER_ACCOUNT}.json"

near_token_owner_credentials_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_sender_account_file = const.near_account_dir / f"{NTA.SENDER_ACCOUNT}.json"
near_bridge_contract_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
near_test_token_file = const.near_deploy_results_dir / f"{NC.MOCK_TOKEN}.json"

call_dir = const.common_generated_dir / "02-transfer-near-to-{network}"

# Main pipeline rule
# TODO: Replace ETH_SEPOLIA with all EVM networks when the `evm_deploy_token` rule doesn't crash on Base and Arbitrum (the issue is not in the pipeline)
rule transfer_near_to_evm_all:
    input:
        expand(call_dir / "verify-transfer-report.txt", 
               network=list([const.EvmNetwork.ETH_SEPOLIA]))
            #    network=list(const.EvmNetwork))
    message: "Transfer Near to EVM pipeline completed"
    default_target: True

rule near_storage_deposit:
    message: "Transfer token from Near to {wildcards.network}. Step 0: Storage deposit for bridge and sender accounts"
    input:
        rules.verify_bridge_token_near_to_evm.output,
        near_sender_account = near_sender_account_file,
        near_bridge_contract = near_bridge_contract_file,
        near_test_token = near_test_token_file,
        near_owner_account = near_token_owner_credentials_file
    output: call_dir / "00_storage-deposit.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        token_id = lambda wc, input: get_json_field(input.near_test_token, "contract_id"),
        sender_account_id = lambda wc, input: get_json_field(input.near_sender_account, "account_id"),
        bridge_account_id = lambda wc, input: get_json_field(input.near_bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.token_id} \
        -m storage_deposit \
        -a '{{\"account_id\": \"{params.sender_account_id}\"}}' \
        -f {input.near_owner_account} \
        -d 0.00235NEAR \
        -n testnet 2>&1 | tee {output} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.token_id} \
        -m storage_deposit \
        -a '{{\"account_id\": \"{params.bridge_account_id}\"}}' \
        -f {input.near_owner_account} \
        -d 0.00235NEAR \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule near_fund_sender:
    message: "Transfer token from Near to {wildcards.network}. Step 1: Fund sender account with test tokens"
    input:
        rules.near_storage_deposit.output,
        near_sender_account = near_sender_account_file,
        near_test_token = near_test_token_file,
        near_owner_account = near_token_owner_credentials_file
    output: call_dir / "01_fund-sender.json"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        token_id = lambda wc, input: get_json_field(input.near_test_token, "contract_id"),
        sender_account_id = lambda wc, input: get_json_field(input.near_sender_account, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.token_id} \
        -m ft_transfer \
        -a '{{\"receiver_id\": \"{params.sender_account_id}\", \"amount\":\"1000000000000\"}}' \
        -f {input.near_owner_account} \
        -d 1YOCTONEAR \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """


rule near_init_transfer:
    message: "Transfer token from Near to {wildcards.network}. Step 2: Init transfer on Near"
    input:
        rules.near_fund_sender.output,
        sender_account = near_sender_account_file,
        bridge_contract = near_bridge_contract_file,
        test_token = near_test_token_file,
        evm_account = evm_account_file
    output: call_dir / "02_init-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        token_id = lambda wc, input: get_json_field(input.test_token, "contract_id"),
        sender_account_id = lambda wc, input: get_json_field(input.sender_account, "account_id"),
        sender_private_key = lambda wc, input: get_json_field(input.sender_account, "private_key"),
        recipient_address = lambda wc, input: get_json_field(input.evm_account, "address"),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
    {params.mkdir} && \
    bridge-cli testnet near-init-transfer \
        --token {params.token_id} \
        --amount 23000000000 \
        --recipient {params.recipient_address} \
        --near-signer {params.sender_account_id} \
        --near-private-key {params.sender_private_key} \
        --near-token-locker-id {params.token_locker_id} \
        --config {params.config_file} > {output} && \
    {params.extract_tx}
    """

rule near_sign_transfer:
    message: "Transfer token from Near to {wildcards.network}. Step 3: Sign transfer on Near"
    input:
        near_init_transfer = rules.near_init_transfer.output,
        sender_account = near_sender_account_file,
        bridge_contract = near_bridge_contract_file,
    output: call_dir / "03_sign-transfer.json"
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
        --tx-hash {params.init_transfer_tx_hash}) && \
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
    message: "Transfer token from Near to {wildcards.network}. Step 4: Fin transfer on EVM"
    input:
        near_sign_transfer = rules.near_sign_transfer.output,
        evm_bridge = evm_bridge_contract_file,
    output: call_dir / "04_fin-transfer.json"
    params:
        config_file = const.common_bridge_sdk_config_file,
        mkdir = get_mkdir_cmd(call_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
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
    message: "Transfer token from Near to {wildcards.network}. Verification"
    input:
        rules.evm_fin_transfer.output,
        test_token = near_test_token_file,
        bridge_contract = near_bridge_contract_file,
        evm_account = evm_account_file
    output: call_dir / "verify-transfer-report.txt"
    params:
        mkdir = get_mkdir_cmd(call_dir),
        call_dir = lambda wildcards: str(call_dir).format(network=wildcards.network),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        token_locker_id = lambda wc, input: get_json_field(input.bridge_contract, "contract_id"),
        near_token_id = lambda wc, input: get_json_field(input.test_token, "contract_id"),
        progress_wait_cmd = progress_wait(5),
        recipient_address = lambda wc, input: get_json_field(input.evm_account, "address"),
    shell: """
    {params.mkdir} && \
    {params.progress_wait_cmd} \
    yarn --cwd {const.common_tools_dir} --silent verify-transfer-near-to-evm \
        --tx-dir {params.call_dir} \
        --receiver {params.recipient_address} \
        --near-token {params.near_token_id} \
        --chain-kind {params.evm_chain_str} \
        --near-locker {params.token_locker_id} \
        | tee {output}
    """
