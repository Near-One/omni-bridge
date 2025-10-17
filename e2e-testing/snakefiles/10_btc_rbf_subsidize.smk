import const
from const import (NearContract as NC, NearTestAccount as NTA)
from utils import get_mkdir_cmd, get_json_field, extract_tx_hash, get_btc_address, get_last_value, get_tx_hash

module near:
    snakefile: "./near.smk"
use rule * from near

module btc_setup:
    snakefile: "./btc_setup.smk"
use rule * from btc_setup

module btc_near_transfer:
    snakefile: "./04_btc_near_transfer.smk"
use rule * from btc_near_transfer

# Directories
call_dir = const.common_generated_dir / "10-btc-rbf-subsidize"

# Account files
near_init_account_file = const.near_account_dir / f"{NTA.INIT_ACCOUNT}.json"
user_account_file = const.near_account_dir / f"{NTA.USER_ACCOUNT}.json"

omni_bridge_file = const.near_deploy_results_dir / f"{NC.OMNI_BRIDGE}.json"
nbtc_file = const.near_deploy_results_dir / f"nbtc.json"
btc_connector_file = const.near_deploy_results_dir / f"btc_connector.json"

rule init_btc_transfer_to_btc:
    message: "Init transfer from Near to BTC"
    input:
        step_0 = rules.fin_btc_transfer_on_near.output,
        btc_connector_file = btc_connector_file,
        btc_file = nbtc_file,
        user_account_file = user_account_file
    output: call_dir / "01_init_btc_transfer_to_btc"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        btc_token = lambda wc, input: get_json_field(input.btc_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
    shell: """
    set -a
    source "$PWD/tools/.env"
    set +a
    
    bridge-cli testnet internal init-near-to-bitcoin-transfer\
        --chain btc \
        --target-btc-address $BTC_ACCOUNT_ID \
        --amount 3000 \
        --fee-rate 500 \
        --btc-connector {params.btc_connector} \
        --btc {params.btc_token} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule sign_btc_transfer:
    message: "Sign BTC transfer"
    input:
       step_1=lambda wc: {
            "02": rules.init_btc_transfer_to_btc.output,
            "05": call_dir / "04_rbf_subsidize",
            "07": call_dir / "06_rbf_subsidize",
        }[wc.step],
       btc_connector_file = btc_connector_file,
       user_account_file = user_account_file
    output: call_dir / "{step}_sign_btc_transfer"
    params:
        near_tx_hash = lambda wc, input: get_tx_hash(input.step_1),
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,

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
    message: "Send btc transfer"
    input:
        btc_connector_file=btc_connector_file,
        user_account_file=user_account_file,
        step=lambda wc: {
            "03": call_dir / "02_sign_btc_transfer",
            "08": call_dir / "07_sign_btc_transfer",
        }[wc.step],
    output:
        call_dir / "{step}_send_btc_transfer"
    params:
        near_tx_hash=lambda wc, input: get_tx_hash(input.step),
        btc_connector=lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id=lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        bridge_sdk_config_file=const.common_bridge_sdk_config_file,
    shell: """
    bridge-cli testnet btc-fin-transfer \
        --chain btc \
        --btc-connector {params.btc_connector} \
        --near-tx-hash {params.near_tx_hash} \
        --relayer {params.user_account_id} \
        --config {params.bridge_sdk_config_file} \
        > {output}
    """

rule rbf_subsidize:
    message: "RBF subsidize"
    input:
        step=lambda wc: {
            "06": call_dir / "05_sign_btc_transfer",
            "04": call_dir / "03_send_btc_transfer",
        }[wc.step],
        step_3 = call_dir / "03_send_btc_transfer",
        btc_connector_file = btc_connector_file,
        btc_file = nbtc_file,
        user_account_file = user_account_file
    output: call_dir / "{step}_rbf_subsidize"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        btc_token = lambda wc, input: get_json_field(input.btc_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_3),
        fee_rate=lambda wc: {
            "06": 3000,
            "04": 2000,
        }[wc.step],
    shell: """
    bridge-cli testnet btc-subsidize-rbf \
        --btc-tx-hash {params.btc_tx_hash} \
        --fee-rate {params.fee_rate} \
        --btc-connector {params.btc_connector} \
        --btc {params.btc_token} \
        --near-signer {params.user_account_id} \
        --near-private-key {params.user_private_key} \
        --config {params.bridge_sdk_config_file} \
         > {output} \
    """

rule verify_withdraw:
    message: "Verify withdraw"
    input:
        step_8 = call_dir / "08_send_btc_transfer",
        btc_connector_file = btc_connector_file,
        nbtc_file = nbtc_file,
        user_account_file = user_account_file
    output: call_dir / "09_verify_withdraw.json"
    params:
        btc_connector = lambda wc, input: get_json_field(input.btc_connector_file, "contract_id"),
        user_account_id = lambda wc, input: get_json_field(input.user_account_file, "account_id"),
        user_private_key = lambda wc, input: get_json_field(input.user_account_file, "private_key"),
        bridge_sdk_config_file = const.common_bridge_sdk_config_file,
        btc_tx_hash = lambda wc, input: get_last_value(input.step_8),
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

rule all:
    input: rules.verify_last_transfer.output
    message: "Transfer NEAR to BTC with RBF pipeline completed"
    default_target: True
