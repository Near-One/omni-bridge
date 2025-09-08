import pathlib
import const
import time
from const import (NearTestAccount as NTA, NearExternalContract as NEC)
from utils import get_mkdir_cmd, get_json_field

module near:
    snakefile: "./near.smk"
use rule * from near

# Account files
near_dao_account_file = const.near_account_dir / f"{NTA.DAO_ACCOUNT}.json"
zcash_connector_account_file = const.near_account_dir / f"{NEC.ZCASH_CONNECTOR}.json"
zcash_account_file = const.near_account_dir / f"{NEC.ZCASH_TOKEN}.json"

rule near_generate_zcash_init_args:
    message: "Generating zcash init args"
    input:
        zcash_connector_account_file = zcash_connector_account_file,
        near_dao_account_file = near_dao_account_file
    output: const.common_generated_dir / "zcash_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller = lambda wc, input: get_json_field(input.near_dao_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    echo '{{\"controller\": \"{params.controller}\", \"bridge_id\":\"{params.bridge_id}\"}}' > {output}
    """

rule near_generate_zcash_connector_init_args:
    message: "Generating btc-connector init args"
    input:
        zcash_connector_account_file = zcash_connector_account_file,
        zcash_account_file = zcash_account_file,
    output: const.common_generated_dir / "zcash_connector_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        zcash_id = lambda wc, input: get_json_field(input.zcash_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.zcash_id} send-near {params.bridge_id} '3 NEAR' network-config testnet sign-with-keychain send &&\
    echo '{{\"config\": {{\"chain\": \"ZcashTestnet\", \"chain_signatures_account_id\": \"v1.signer-prod.testnet\",\"nbtc_account_id\": \"{params.zcash_id}\",\"btc_light_client_account_id\": \"zcash-client.n-bridge.testnet\",\"confirmations_strategy\": {{\"100000000\": 6}},\"confirmations_delta\": 1,\"withdraw_bridge_fee\": {{\"fee_min\": \"400\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"deposit_bridge_fee\": {{\"fee_min\": \"200\",\"fee_rate\": 0,\"protocol_fee_rate\": 9000}},\"min_deposit_amount\": \"500\", \"min_withdraw_amount\": \"500\", \"min_change_amount\": \"0\", \"max_change_amount\": \"100000000\",\"min_btc_gas_fee\": \"100\",\"max_btc_gas_fee\": \"80000\",\"max_withdrawal_input_number\": 10,\"max_change_number\": 10,\"max_active_utxo_management_input_number\": 10,\"max_active_utxo_management_output_number\": 10,\"active_management_lower_limit\": 0,\"active_management_upper_limit\": 1000,\"passive_management_lower_limit\": 0,\"passive_management_upper_limit\": 600,\"rbf_num_limit\": 99,\"max_btc_tx_pending_sec\": 86400, \"expiry_height_gap\": 1000}}}}' > {output}
    """
