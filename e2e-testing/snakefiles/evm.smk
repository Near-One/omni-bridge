import pathlib
import const
from const import EvmContract as EC, NearContract as NC, EvmNetwork as EN, get_evm_deploy_results_dir, get_evm_account_dir
from utils import get_json_field

module near_module:
    snakefile: "near.smk"
use rule * from near_module

EVM_NETWORKS = [network for network in EN]

evm_dir = const.common_testing_root / "../evm"
evm_build_stamp = const.common_generated_dir / ".evm_compile.stamp"
evm_artifacts_dir = const.common_generated_dir / "evm_artifacts"
evm_enear_creation_template_file = const.common_testing_root / "bin" / "eNear_creation.template"

near_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"

enear_creation_file = "eNear_creation"
network_deployed_stamp = ".deployed"

def evm_deploy_token_impl(network):
    return f"yarn --silent --cwd {evm_dir} hardhat deploy-token-impl --network {network}"

def evm_deploy_bridge_contract(network, token_impl_addr, near_bridge_id):
    return f"yarn --silent --cwd {evm_dir} hardhat deploy-bridge-token-factory --network {network} --bridge-token-impl {token_impl_addr} --near-bridge-account-id {near_bridge_id}"

def evm_deploy_fake_prover(network):
    return f"yarn --silent --cwd {evm_dir} hardhat deploy-fake-prover --network {network}"

def evm_deploy_enear_proxy(network, enear_addr):
    return f"yarn --silent --cwd {evm_dir} hardhat deploy-e-near-proxy --network {network} --enear {enear_addr}"

def evm_deploy_bytecode(network, bytecode_file):
    return f"yarn --silent --cwd {const.common_tools_dir} hardhat deploy-bytecode --network {network} --bytecode {bytecode_file}"

def evm_deploy_test_token(network, name, symbol):
    return f"yarn --silent --cwd {const.common_tools_dir} hardhat deploy-test-token --network {network} --name {name} --symbol {symbol}"

def evm_create_eoa(network):
    return f"yarn --silent --cwd {const.common_tools_dir} hardhat create-eoa --network {network}"

def evm_get_current_eoa(network):
    return f"yarn --silent --cwd {const.common_tools_dir} hardhat get-current-eoa --network {network}"

def get_mkdir_cmd(wildcards):
    return f"mkdir -p {get_evm_deploy_results_dir(wildcards.network)}"

def get_full_path(file_name):
    return f"{get_evm_deploy_results_dir('{network}')}/{file_name}"


# Rule to deploy all networks
rule evm_deploy_all:
    input:
        f"{get_evm_deploy_results_dir(EN.ETH_SEPOLIA)}/{network_deployed_stamp}",
        f"{get_evm_deploy_results_dir(EN.ARBITRUM_SEPOLIA)}/{network_deployed_stamp}",
        f"{get_evm_deploy_results_dir(EN.BASE_SEPOLIA)}/{network_deployed_stamp}",
    default_target: True


rule evm_build:
    message: "Building EVM contracts"
    output: evm_build_stamp
    shell: f"""
    mkdir -p {evm_artifacts_dir} && \
    yarn --cwd {evm_dir} install --frozen-lockfile && \
    yarn --cwd {evm_dir} hardhat compile && \
    cp -r {evm_dir}/build/* {evm_artifacts_dir} && \
    touch {{output}}
    """

rule evm_create_eoa_account:
    message: "Creating EOA account"
    output: pathlib.Path(get_evm_account_dir("{network}")) / f"{EC.USER_ACCOUNT}.json"
    params:
        evm_account_dir = lambda wc: get_evm_account_dir(wc.network),
        create_cmd = lambda wc: evm_get_current_eoa(wc.network)
    shell: """
    mkdir -p {params.evm_account_dir} && \
    {params.create_cmd} 2>/dev/stderr 1> {output}
    """

rule evm_deploy_fake_prover:
    message: "Deploying fake prover to {wildcards.network}"
    input:
        build_stamp = evm_build_stamp
    output: get_full_path(f"{EC.FAKE_PROVER}.json")
    params:
        mkdir = get_mkdir_cmd,
        deploy_cmd = lambda wc: evm_deploy_fake_prover(wc.network)
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


rule evm_create_enear_creation_file:
    message: "Creating eNear creation file for {wildcards.network}"
    input:
        fake_prover = rules.evm_deploy_fake_prover.output,
        template = evm_enear_creation_template_file
    output: get_full_path(enear_creation_file)
    params:
        mkdir = get_mkdir_cmd
    shell: """
    {params.mkdir} && \
    cat {input.template} | \
    sed "s/<PROVER_ADDRESS>/$(cat {input.fake_prover} | jq -r .fakeProverAddress | sed 's/^0x//')/" > {output}
    """


rule evm_deploy_enear:
    message: "Deploying eNear to {wildcards.network}"
    input:
        creation_file = get_full_path(enear_creation_file),
        tools_stamp = const.common_tools_compile_stamp
    output: get_full_path(f"{EC.ENEAR}.json")
    params:
        mkdir = get_mkdir_cmd,
        deploy_cmd = lambda wildcards: evm_deploy_bytecode(wildcards.network, input.creation_file)
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


rule evm_deploy_enear_proxy:
    message: "Deploying eNear proxy to {wildcards.network}"
    input:
        rules.evm_build.output,
        enear = rules.evm_deploy_enear.output
    output: get_full_path(f"{EC.ENEAR_PROXY}.json")
    params:
        mkdir=get_mkdir_cmd,
        deploy_cmd=lambda wc, input: evm_deploy_enear_proxy(wc.network, get_json_field(input.enear, "contractAddress"))
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


rule evm_deploy_token_impl:
    message: "Deploying token implementation to {wildcards.network}"
    input:
        rules.evm_build.output
    output: get_full_path(f"{EC.TOKEN_IMPL}.json")
    params:
        mkdir = get_mkdir_cmd,
        deploy_cmd = lambda wc: evm_deploy_token_impl(wc.network)
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


rule evm_deploy_bridge:
    message: "Deploying bridge contract to {wildcards.network}"
    input:
        rules.evm_build.output,
        token_impl = rules.evm_deploy_token_impl.output,
        near_bridge_file = near_bridge_file,
    output: get_full_path(f"{EC.OMNI_BRIDGE}.json")
    params:
        mkdir = get_mkdir_cmd,
        deploy_cmd = lambda wc, input: evm_deploy_bridge_contract(wc.network,
                                                                get_json_field(input.token_impl, "tokenImplAddress"),
                                                                get_json_field(input.near_bridge_file, "contract_id"))
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


rule evm_deploy_test_token:
    message: "Deploying test token to {wildcards.network}"
    input:
        const.common_tools_compile_stamp
    output: get_full_path(f"{EC.TEST_TOKEN}.json")
    params:
        mkdir = get_mkdir_cmd,
        deploy_cmd = lambda wc: evm_deploy_test_token(wc.network,
                                                    f"E2ETestToken-{const.common_timestamp}",
                                                    f"E2ETT-{const.common_timestamp}")
    shell: """
    {params.mkdir} && \
    {params.deploy_cmd} 2>/dev/stderr 1> {output}
    """


# Aggregate rules for each network
rule evm_deploy_to_network:
    message: "Deploying network {wildcards.network}"
    input:
        bridge = rules.evm_deploy_bridge.output,
        enear_proxy = rules.evm_deploy_enear_proxy.output,
        test_token = rules.evm_deploy_test_token.output
    output:
        touch(f"{const.evm_deploy_results_dir}/{{network}}/{network_deployed_stamp}")
    params:
        mkdir = get_mkdir_cmd
    shell: """
    {params.mkdir}
    touch {output}
    """

rule deploy_sepolia:
    input:
        f"{const.evm_deploy_results_dir}/{EN.ETH_SEPOLIA}/{network_deployed_stamp}"

rule deploy_arbitrumSepolia:
    input:
        f"{const.evm_deploy_results_dir}/{EN.ARBITRUM_SEPOLIA}/{network_deployed_stamp}"

rule deploy_baseSepolia:
    input:
        f"{const.evm_deploy_results_dir}/{EN.BASE_SEPOLIA}/{network_deployed_stamp}"

