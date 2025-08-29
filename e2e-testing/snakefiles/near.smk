import pathlib
import const
from const import NearContract as NC, NearTestAccount as NTA, NearExternalContract as NEC
from utils import get_json_field, extract_tx_hash

module common_module:
    snakefile: "common.smk"
use rule * from common_module

# NEAR-specific variables and paths
near_dir = const.common_testing_root / "../near"
near_binary_dir = const.near_binary_dir
near_init_params_file = const.near_init_params_file

# All expected WASM binaries
near_binaries = [f"{contract}.wasm" for contract in NC]

# List of binaries that require dynamic init args
near_contracts_with_dynamic_args = [NC.TOKEN_DEPLOYER, NC.MOCK_TOKEN, NC.OMNI_BRIDGE, NEC.ZCASH_CONNECTOR, NEC.ZCASH_TOKEN]

# Account credential files
near_init_account_credentials_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_credentials_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"

# Call result files
near_prover_dau_grant_call_file = const.near_deploy_results_dir / "omni-prover-dau-grant-call.json"
near_evm_prover_setup_call_file = const.near_deploy_results_dir / "{network}-evm-prover-setup-call.json"

# Contract / account files
omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
token_deployer_file = const.near_deploy_results_dir / f"{NC.TOKEN_DEPLOYER}.json"
mock_token_file = const.near_deploy_results_dir / f"{NC.MOCK_TOKEN}.json"
evm_prover_file = const.near_deploy_results_dir / f"{NC.EVM_PROVER}.json"


def get_dyn_init_args_path(contract_name):
    return f"{const.common_generated_dir}/{contract_name}_dyn_init_args.json"

def get_mkdir_cmd(directory):
    return f"mkdir -p {directory}"


rule near_deploy_all:
    message: "Deploying all NEAR contracts"
    input:
        expand(const.near_deploy_results_dir / "{contract}.json",
               contract=[binary.replace(".wasm", "") for binary in near_binaries]),
        near_prover_dau_grant_call_file,
        expand(near_evm_prover_setup_call_file, network=list(const.EvmNetwork))
    default_target: True


rule near_build:
    message: "Building NEAR contracts"
    output:
        expand(near_binary_dir / "{binary}", binary=near_binaries)
    shell: f"""
    OUT_DIR={near_binary_dir} make -f {const.common_testing_root}/../Makefile rust-build-near
    """

rule near_create_account:
    message: "Creating {wildcards.account} account"
    output: const.near_account_dir / "{account}.json"
    params:
        mkdir = get_mkdir_cmd(const.near_account_dir),
        scripts_dir = const.common_scripts_dir,
        ts = const.common_timestamp
    shell: """
    {params.mkdir} && \
    {params.scripts_dir}/create-near-account.sh {wildcards.account}-{params.ts}.testnet {output}
    """


rule near_generate_token_deployer_init_args:
    message: "Generating token deployer init args"
    input:
        omni_bridge = omni_bridge_file,
        init_account =near_init_account_credentials_file
    output: const.common_generated_dir / "token_deployer_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller_address = lambda wc, input: get_json_field(input.omni_bridge, "contract_id"),
        dao_address = lambda wc, input: get_json_field(input.init_account, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"controller\": \"{params.controller_address}\", \"dao\": \"{params.dao_address}\"}}' > {output}
    """


rule near_generate_mock_token_init_args:
    message: "Generating mock token init args"
    input: near_init_account_credentials_file
    output: const.common_generated_dir / "mock_token_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        owner_address = lambda wc, input: get_json_field(input, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"owner_id\": \"{params.owner_address}\"}}' > {output}
    """


rule near_evm_prover_setup:
    message: "Setting up EVM prover"
    input:
        omni_bridge = omni_bridge_file,
        evm_prover = evm_prover_file,
        dao_account = near_dao_account_credentials_file
    output: near_evm_prover_setup_call_file
    params:
        mkdir = get_mkdir_cmd(const.near_deploy_results_dir),
        evm_chain_str = lambda wc: const.Chain.from_evm_network(wc.network),
        controller_address = lambda wc, input: get_json_field(input.omni_bridge, "contract_id"),
        evm_prover_account_id = lambda wc, input: get_json_field(input.evm_prover, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
    {params.mkdir} && \
    {const.common_scripts_dir}/call-near-contract.sh -c {params.controller_address} \
        -m add_prover \
        -a '{{\"account_id\": \"{params.evm_prover_account_id}\", \"prover_id\": \"{params.evm_chain_str}\"}}' \
        -f {input.dao_account} \
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """


rule near_deploy_contract:
    message: "Deploying {wildcards.contract} contract"
    input:
        init_params=near_init_params_file,
        init_account=near_init_account_credentials_file,
        binary=near_binary_dir / "{contract}.wasm",
        contract_account = const.near_account_dir / "{contract}.json",
        dyn_args=(lambda wc: const.common_generated_dir / f"{wc.contract}_dyn_init_args.json" if wc.contract in near_contracts_with_dynamic_args else []),
    output: const.near_deploy_results_dir / "{contract}.json"
    params:
        mkdir=get_mkdir_cmd(const.near_deploy_results_dir),
        base_name="{contract}",
        scripts_dir=const.common_scripts_dir,
        ts=const.common_timestamp,
        contract_id = lambda wc, input: get_json_field(input.contract_account, "account_id")
    shell: """
    {params.mkdir} && \
    if [ -f {input.dyn_args} ]; then
        {params.scripts_dir}/deploy-near-contract.sh {input.init_params} {input.init_account} {input.dyn_args} {input.binary} {params.contract_id} {output}
    else
        {params.scripts_dir}/deploy-near-contract.sh {input.init_params} {input.init_account} {input.binary} {params.contract_id} {output}
    fi
    """
