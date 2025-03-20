import pathlib
import const
import time
from const import (get_evm_deploy_results_dir, get_evm_account_dir,
                     EvmNetwork as EN, NearContract as NC, EvmContract as EC, NearTestAccount as NTA)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash

module evm:
    snakefile: "./evm.smk"
use rule * from evm

module near:
    snakefile: "./near.smk"
use rule * from near

# Directories
call_dir = const.common_generated_dir / "02-bridge-migration"
sepolia_call_dir = pathlib.Path(get_evm_deploy_results_dir(EN.ETH_SEPOLIA))
sepolia_account_dir = pathlib.Path(get_evm_account_dir(EN.ETH_SEPOLIA))

# NEAR contract deployment
rainbow_bridge_token_factory_file = const.near_deploy_results_dir / f"{NC.RB_BRIDGE_TOKEN_FACTORY}.json"
omni_bridge_contract_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"

# EVM contract deployment
eth_omni_bridge_contract_file = sepolia_call_dir / f"{EC.OMNI_BRIDGE}.json"
sepolia_test_token_address_file = sepolia_call_dir / f"{EC.TEST_TOKEN}.json"

# Account files
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"
eth_user_account_file = sepolia_account_dir / f"{NTA.USER_ACCOUNT}.json"
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
near_relayer_account_file = const.near_account_dir / f"{NTA.RELAYER_ACCOUNT}.json"

# Call files
bridge_factory_controller_grant_omni_init_call_file = call_dir / "01_bridge-factory-controller-grant-omni-init-call.json"
omni_bridge_dao_grant_call_file = call_dir / "02_omni-bridge-dao-grant-call.json"
# Consider moving to contract deployment section
rb_bridge_token_deploy_file = call_dir / "03_rb-bridge-token-deploy-call.json"
rb_bridge_token_set_metadata_call_file = call_dir / "04_rb-bridge-token-set-metadata-call.json"
bridge_token_controller_init_call_file = call_dir / "05_bridge-token-controller-init-call.json"
mint_token_to_user_near_call_file = call_dir / "06_mint-token-to-user-near-call.json"
bridge_factory_dao_grant_call_file = call_dir / "08_bridge-factory-controller-grant-omni-init-call.json"
bridge_factory_controller_grant_omni_bridge_call_file = call_dir / "09_bridge-factory-controller-grant-omni-bridge-call.json"
bridge_token_controller_omni_bridge_call_file = call_dir / "10_bridge-token-controller-omni-bridge-call.json"
add_token_to_omni_bridge_call_file = call_dir / "11_add-token-to-omni-bridge-call.json"
mint_token_to_omni_bridge_eth_call_file = call_dir / "12_mint-token-to-omni-bridge-eth-call.json"
omni_bridge_storage_deposit_call_file = call_dir / "13_omni-bridge-storage-deposit-call.json"
withdraw_token_on_near_call_file = call_dir / "14_withdraw-token-on-near-call.json"
sign_withdraw_token_on_near_call_file = call_dir / "15_sign-withdraw-token-on-near-call.json"
eth_fin_transfer_call_file = call_dir / "16_eth-fin-transfer-call.json"

correctness_report_file = call_dir / "correctness-report.txt"

# Variables
truncated_timestamp = time.strftime("%Y%m%d%H%M")
rb_bridge_token_deploy_deposit = "6 NEAR"
omni_bridge_add_token_deposit = "0.00125 NEAR"
test_token_amount = 100
storage_deposit = "0.00125 NEAR"


# Binary files
rb_bridge_token_binary_file = const.near_binary_dir / "bridge_token.wasm"
rb_bridge_token_factory_binary_file = const.near_binary_dir / "rb-bridge-token-factory.wasm"


rule get_rb_bridge_token_binary_file:
    output: rb_bridge_token_binary_file
    params:
        mkdir_cmd = get_mkdir_cmd(const.near_binary_dir)
    shell: """
    {params.mkdir_cmd} && \
    wget https://github.com/Near-One/rainbow-token-connector/raw/refs/heads/master/res/bridge_token.wasm -O {output}
    """


rule get_rb_bridge_token_factory_binary_file:
    output: rb_bridge_token_factory_binary_file
    params:
        mkdir_cmd = get_mkdir_cmd(const.near_binary_dir)
    shell: """
    {params.mkdir_cmd} && \
    wget https://github.com/Near-One/rainbow-token-connector/raw/refs/heads/master/res/bridge_token_factory.wasm -O {output}
    """

rule deploy_rb_bridge_token_factory:
    message: "Deploying Rainbow Bridge Token Factory"
    input:
        init_account = near_init_account_file,
        init_params = const.near_init_params_file,
        rb_bridge_token_factory_binary = rb_bridge_token_factory_binary_file,
        rb_bridge_token_binary = rb_bridge_token_binary_file,
    output: rainbow_bridge_token_factory_file
    params:
        mkdir_cmd = get_mkdir_cmd(const.near_deploy_results_dir),
        scripts_dir = const.common_scripts_dir,
        ts = truncated_timestamp
    shell: """
    {params.mkdir_cmd} && \
    export BRIDGE_TOKEN_BINARY={input.rb_bridge_token_binary} && \
    {params.scripts_dir}/deploy-near-contract.sh {input.init_params} {input.init_account} {input.rb_bridge_token_factory_binary} btf{params.ts}.testnet {output}
    """

rule bridge_factory_dao_grant:
    message: "Granting DAO role to Bridge Factory"
    input:
        init_account = near_init_account_file,
        dao_account = near_dao_account_file,
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output
    output:
        bridge_factory_dao_grant_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        rb_bridge_token_factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        dao_account_id = lambda wc, input: get_json_field(input.dao_account, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.rb_bridge_token_factory_address} \
        -m acl_grant_role \
        -a '{{\"role\": \"DAO\", \"account_id\": \"{params.dao_account_id}\"}}' \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule deploy_rb_bridge_token:
    message: "Deploying Rainbow Bridge Token"
    input:
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output,
        sepolia_test_token = sepolia_test_token_address_file,
        init_account = near_init_account_file
    output:
        rb_bridge_token_deploy_file
    params:
        scripts_dir = const.common_scripts_dir,
        rb_bridge_token_factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        sepolia_test_token_address = lambda wc, input: get_json_field(input.sepolia_test_token, "contractAddress")[2:],
        token_contract_id = lambda wc, input: f"{get_json_field(input.sepolia_test_token, 'contractAddress')[2:]}.{get_json_field(input.rb_bridge_token_factory_file, 'contract_id')}".lower(),
        storage_deposit = rb_bridge_token_deploy_deposit,
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.rb_bridge_token_factory_address} \
        -m deploy_bridge_token \
        -a '{{\"address\": \"{params.sepolia_test_token_address}\"}}' \
        -f {input.init_account} \
        -d "{params.storage_deposit}" \
        -n testnet 2>&1 | tee {output} && \
        TX_HASH=$(grep -o 'Transaction ID: [^ ]*' {output} | cut -d' ' -f3) && \
        echo '{{\"tx_hash\": \"'$TX_HASH'\", \"contract_id\": \"{params.token_contract_id}\"}}' > {output}
    """

rule set_bridge_token_metadata:
    message: "Setting metadata for Rainbow Bridge Token"
    input:
        rules.bridge_factory_dao_grant.output,
        rules.deploy_rb_bridge_token.output,
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output,
        sepolia_test_token = sepolia_test_token_address_file,
        dao_account = near_dao_account_file
    output:
        rb_bridge_token_set_metadata_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        rb_bridge_token_factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        sepolia_test_token_address = lambda wc, input: get_json_field(input.sepolia_test_token, "contractAddress")[2:],
        metadata_token_name = lambda wc, input: get_json_field(input.sepolia_test_token, "name"),
        metadata_token_symbol = lambda wc, input: get_json_field(input.sepolia_test_token, "symbol"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.rb_bridge_token_factory_address} \
        -m set_metadata \
        -a '{{\"address\": \"{params.sepolia_test_token_address}\", \"name\": \"{params.metadata_token_name}\", \"symbol\": \"{params.metadata_token_symbol}\", \"decimals\": 18}}' \
        -f {input.dao_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule bridge_factory_controller_grant_omni_init:
    message: "Granting Controller role of Bridge Factory to Omni Init"
    input:
        rules.set_bridge_token_metadata.output,
        init_account = near_init_account_file,
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output
    output:
        bridge_factory_controller_grant_omni_init_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        rb_bridge_token_factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        factory_controller_address = lambda wc, input: get_json_field(input.init_account, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.rb_bridge_token_factory_address} \
        -m acl_grant_role \
        -a '{{\"role\": \"Controller\", \"account_id\": \"{params.factory_controller_address}\"}}' \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule omni_bridge_dao_grant:
    message: "Granting DAO role of Omni Bridge to DAO account"
    input:
        init_account = near_init_account_file,
        dao_account = near_dao_account_file,
        omni_bridge_contract_file = omni_bridge_contract_file
    output:
        omni_bridge_dao_grant_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        dao_account_id = lambda wc, input: get_json_field(input.dao_account, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_address} \
        -m acl_grant_role \
        -a '{{\"role\": \"DAO\", \"account_id\": \"{params.dao_account_id}\"}}' \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule set_bridge_token_controller_init:
    message: "Granting controller role of bridge token to omni-init"
    input:
        rules.bridge_factory_controller_grant_omni_init.output,
        rb_bridge_token_deploy_file = rules.deploy_rb_bridge_token.output,
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output,
        init_account = near_init_account_file
    output:
        bridge_token_controller_init_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        token_address = lambda wc, input: get_json_field(input.rb_bridge_token_deploy_file, "contract_id"),
        factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.factory_address} \
        -m set_controller_for_tokens \
        -a '{{\"tokens_account_id\": [\"{params.token_address}\"]}}' \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule mint_token_to_user_near:
    message: "Minting token to NEAR user"
    input:
        rules.set_bridge_token_controller_init.output,
        rules.set_bridge_token_metadata.output,
        rb_bridge_token_deploy_file = rules.deploy_rb_bridge_token.output,
        user_account = user_account_file,
        init_account = near_init_account_file
    output:
        mint_token_to_user_near_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        token_address = lambda wc, input: get_json_field(input.rb_bridge_token_deploy_file, "contract_id"),
        user_address = lambda wc, input: get_json_field(input.user_account, "account_id"),
        token_amount = test_token_amount,
        storage_deposit = storage_deposit,
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.token_address} \
        -m mint \
        -a '{{\"amount\": \"{params.token_amount}\", \"account_id\": \"{params.user_address}\"}}' \
        -f {input.init_account} \
        -d "{params.storage_deposit}" \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule bridge_factory_controller_grant_omni_bridge:
    message: "Granting controller role of bridge factory to Omni Bridge"
    input:
        rules.mint_token_to_user_near.output,
        init_account = near_init_account_file,
        omni_bridge_contract_file = omni_bridge_contract_file,
        rb_bridge_token_factory_file = rules.deploy_rb_bridge_token_factory.output
    output:
        bridge_factory_controller_grant_omni_bridge_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        bridge_factory_address = lambda wc, input: get_json_field(input.rb_bridge_token_factory_file, "contract_id"),
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.bridge_factory_address} \
        -m acl_grant_role \
        -a '{{\"role\": \"Controller\", \"account_id\": \"{params.omni_bridge_address}\"}}' \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule bridge_token_controller_omni_bridge:
    message: "Granting controller role of bridge token to Omni Bridge"
    input:
        rules.bridge_factory_controller_grant_omni_bridge.output,
        rb_bridge_token_deploy_file = rules.deploy_rb_bridge_token.output,
        omni_bridge_contract_file = omni_bridge_contract_file,
        init_account = near_init_account_file
    output:
        bridge_token_controller_omni_bridge_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        token_address = lambda wc, input: get_json_field(input.rb_bridge_token_deploy_file, "contract_id"),
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.token_address} \
        -m set_controller \
        -a '{{\"controller\": \"{params.omni_bridge_address}\"}}' \
        -f {input.init_account} \
        -g "300.0 Tgas" \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule add_token_to_omni_bridge:
    message: "Adding token to Omni Bridge"
    input:
        rb_bridge_token_deploy_file = rules.deploy_rb_bridge_token.output,
        omni_bridge_dao_grant = rules.omni_bridge_dao_grant.output,
        omni_bridge_contract_file = omni_bridge_contract_file,
        sepolia_test_token = sepolia_test_token_address_file,
        dao_account = near_dao_account_file
    output:
        add_token_to_omni_bridge_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        token_address = lambda wc, input: get_json_field(input.rb_bridge_token_deploy_file, "contract_id"),
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        eth_token_address = lambda wc, input: f"eth:{get_json_field(input.sepolia_test_token, 'contractAddress')}",
        omni_bridge_add_token_deposit = omni_bridge_add_token_deposit,
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_address} \
        -m add_deployed_tokens \
        -a '{{\"tokens\": [{{\"token_address\": \"{params.eth_token_address}\", \"token_id\": \"{params.token_address}\", \"decimals\": 18}}]}}' \
        -f {input.dao_account} \
        -d "{params.omni_bridge_add_token_deposit}" \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule mint_token_to_omni_bridge_eth:
    message: "Minting token to Eth Omni Bridge"
    input:
        eth_omni_bridge_contract = eth_omni_bridge_contract_file,
        sepolia_test_token = sepolia_test_token_address_file
    output:
        mint_token_to_omni_bridge_eth_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.eth_omni_bridge_contract, "bridgeAddress"),
        token_address = lambda wc, input: get_json_field(input.sepolia_test_token, "contractAddress"),
        token_amount = test_token_amount
    shell: """
    yarn --silent --cwd {const.common_tools_dir} hardhat mint-test-token \
        --network sepolia \
        --contract {params.token_address} \
        --to {params.omni_bridge_address} \
        --amount {params.token_amount} > {output}
    """

rule omni_bridge_storage_deposit:
    message: "Depositing storage for Omni Bridge"
    input:
        omni_bridge_contract_file = omni_bridge_contract_file,
        init_account = near_init_account_file
    output:
        omni_bridge_storage_deposit_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_address} \
        -m storage_deposit \
        -a '{{\"account_id\": \"{params.omni_bridge_address}\"}}' \
        -d "1 NEAR" \
        -f {input.init_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule withdraw_token_on_near:
    message: "Withdrawing token on NEAR"
    input:
        rules.mint_token_to_omni_bridge_eth.output,
        rules.omni_bridge_storage_deposit.output,
        rules.mint_token_to_user_near.output,
        rules.add_token_to_omni_bridge.output,
        rules.bridge_token_controller_omni_bridge.output,
        rb_bridge_token_deploy_file = rules.deploy_rb_bridge_token.output,
        user_account = user_account_file,
        eth_user_account = eth_user_account_file
    output:
        withdraw_token_on_near_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        token_address = lambda wc, input: get_json_field(input.rb_bridge_token_deploy_file, "contract_id"),
        recipient_address = lambda wc, input: get_json_field(input.eth_user_account, "address"),
        token_amount = test_token_amount,
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.token_address} \
        -m withdraw \
        -a '{{\"amount\": \"{params.token_amount}\", \"recipient\": \"{params.recipient_address}\"}}' \
        -f {input.user_account} \
        -d "1 yoctoNEAR" \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule sign_withdraw_token_on_near:
    message: "Signing withdraw token on NEAR"
    input:
        rules.withdraw_token_on_near.output,
        near_relayer_account = near_relayer_account_file,
        omni_bridge_contract_file = omni_bridge_contract_file
    output:
        sign_withdraw_token_on_near_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        relayer_address = lambda wc, input: get_json_field(input.near_relayer_account, "account_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.scripts_dir}/call-near-contract.sh -c {params.omni_bridge_address} \
        -m sign_transfer \
        -a '{{\"transfer_id\": {{\"origin_chain\": \"Near\", \"origin_nonce\": 1}}, \"fee_recipient\": \"{params.relayer_address}\", \"fee\": {{\"fee\": \"0\", \"native_fee\": \"0\"}}}}' \
        -f {input.near_relayer_account} \
        -g "300.0 Tgas" \
        -d "1 yoctoNEAR" \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """

rule eth_fin_transfer:
    message: "Finalizing transfer on Ethereum"
    input:
        sign_withdraw_token_on_near = rules.sign_withdraw_token_on_near.output,
        eth_omni_bridge_contract = eth_omni_bridge_contract_file,
        omni_bridge_contract_file = omni_bridge_contract_file,
        init_account = near_init_account_file
    output:
        eth_fin_transfer_call_file
    params:
        scripts_dir = const.common_scripts_dir,
        omni_bridge_address = lambda wc, input: get_json_field(input.omni_bridge_contract_file, "contract_id"),
        eth_bridge_token_factory_address = lambda wc, input: get_json_field(input.eth_omni_bridge_contract, "bridgeAddress"),
        tx_hash = lambda wc, input: get_json_field(input.sign_withdraw_token_on_near, "tx_hash"),
        init_account_id = lambda wc, input: get_json_field(input.init_account, "account_id"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        sepolia_chain_str = const.Chain.ETH,
        extract_tx = lambda wc, output: extract_tx_hash("bridge", output)
    shell: """
    bridge-cli testnet evm-fin-transfer \
        --chain {params.sepolia_chain_str} \
        --tx-hash {params.tx_hash} \
        --near-token-locker-id {params.omni_bridge_address} \
        --eth-bridge-token-factory-address {params.eth_bridge_token_factory_address} \
        --near-signer {params.init_account_id} \
        --config {params.bridge_sdk_config_file} > {output} && \
    {params.extract_tx}
    """

rule verify_correctness:
    message: "Verifying correctness of the pipeline"
    input:
        eth_fin_transfer = rules.eth_fin_transfer.output,
        sepolia_test_token = sepolia_test_token_address_file,
        eth_user_account = eth_user_account_file,
        tools_compile_stamp = const.common_tools_compile_stamp
    output:
        correctness_report_file
    params:
        token_address = lambda wc, input: get_json_field(input.sepolia_test_token, "contractAddress"),
        recipient_address = lambda wc, input: get_json_field(input.eth_user_account, "address"),
        token_amount = test_token_amount,
        tools_dir = const.common_tools_dir,
        call_dir = call_dir
    shell: """
    # Wait a bit for Ethereum fin-transfer to be completed
    sleep 10 && \
    
    yarn --cwd {params.tools_dir} --silent verify-pipeline-2 \
        --tx-dir {params.call_dir} \
        --token {params.token_address} \
        --account {params.recipient_address} \
        --balance {params.token_amount} | tee {output}
    """

rule all:
    input:
        rules.verify_correctness.output
    default_target: True
