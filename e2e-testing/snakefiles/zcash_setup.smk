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
        zcash_account_file = zcash_account_file,
        near_dao_account_file = near_dao_account_file
    output: const.common_generated_dir / "zcash_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller = lambda wc, input: get_json_field(input.near_dao_account_file, "account_id"),
        zcash_id = lambda wc, input: get_json_field(input.zcash_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.controller} send-near {params.zcash_id} '2 NEAR' network-config testnet sign-with-keychain send &&\
    echo '{{\"controller\": \"{params.controller}\", \"bridge_id\":\"{params.bridge_id}\"}}' > {output}
    """

rule near_generate_zcash_connector_init_args:
    message: "Generating btc-connector init args"
    input:
        zcash_connector_account_file = zcash_connector_account_file,
        zcash_account_file = zcash_account_file,
        near_dao_account_file = near_dao_account_file
    output: const.common_generated_dir / "zcash_connector_dyn_init_args.json"
    params:
        mkdir = get_mkdir_cmd(const.common_generated_dir),
        controller = lambda wc, input: get_json_field(input.near_dao_account_file, "account_id"),
        zcash_id = lambda wc, input: get_json_field(input.zcash_account_file, "account_id"),
        bridge_id = lambda wc, input: get_json_field(input.zcash_connector_account_file, "account_id")
    shell: """
    {params.mkdir} && \
    near tokens {params.controller} send-near {params.bridge_id} '3 NEAR' network-config testnet sign-with-keychain send &&\
    echo '{{\"config\": {{\"nbtc_account_id\": \"{params.zcash_id}\"}}}}' > {output}
    """
