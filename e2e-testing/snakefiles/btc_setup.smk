import const
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash
from const import (NearContract as NC, NearTestAccount as NTA)

# Binary files
btc_connector_binary_file = const.near_binary_dir / "btc_connector.wasm"
nbtc_binary_file = const.near_binary_dir / "nbtc.wasm"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
btc_connector_account_file = const.near_account_dir / f"btc_connector.json"
nbtc_account_file = const.near_account_dir / f"nbtc.json"

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

rule near_fund_btc_connector:
    message: "Transfer Near Tokens to btc-connector"
    input:
        btc_connector_account_file = btc_connector_account_file,
        nbtc_account_file = nbtc_account_file,
    output: const.common_generated_dir / "fund_btc_connector.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        nbtc_id = lambda wc, input: get_json_field(input.nbtc_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.btc_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.nbtc_id} send-near {params.bridge_id} '2 NEAR' network-config testnet sign-with-keychain send > {output}
    """

rule near_generate_btc_connector_init_args:
    message: "Generating btc-connector init args"
    input:
        rules.near_fund_btc_connector.output,
        btc_connector_account_file = btc_connector_account_file,
        nbtc_account_file = nbtc_account_file,
    output: const.common_generated_dir / "btc_connector_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        nbtc_id = lambda wc, input: get_json_field(input.nbtc_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.btc_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"config\": {{\"nbtc_account_id\": \"{params.nbtc_id}\"}}}}' > {output}
    """

rule sync_btc_connector:
    message: "Sync BTC connector"
    input:
        btc_connector_file = btc_connector_file,
        init_account_file = near_init_account_file
    output: const.common_generated_dir / "sync_btc_connector.json"
    params:
        scripts_dir = const.common_scripts_dir,
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        extract_tx = lambda wc, output: extract_tx_hash("near", output)
    shell: """
        {params.scripts_dir}/call-near-contract.sh -c {params.btc_connector} \
        -m sync_chain_signatures_root_public_key \
        -f {input.init_account_file} \
        -d "1 yoctoNEAR"\
        -n testnet 2>&1 | tee {output} && \
    {params.extract_tx}
    """


