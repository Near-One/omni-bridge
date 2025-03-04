from enum import StrEnum
from pathlib import Path
from datetime import datetime


# Common
common_testing_root = Path(__file__).parent.parent
common_generated_dir = common_testing_root / "generated"
common_tools_dir = common_testing_root / "tools"
common_scripts_dir = common_tools_dir / "src" /"scripts"
common_tools_compile_stamp = common_generated_dir / "common_tools_compile.stamp"
common_timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
common_bridge_sdk_config_file = common_testing_root / "bridge-sdk-config.json"

# Near
near_deploy_results_dir = common_generated_dir / "near_deploy_results"
near_account_dir = common_generated_dir / "near_accounts"

# EVM
evm_deploy_results_dir = common_generated_dir / "evm_deploy_results"


class Chain(StrEnum):
    ETH = "Eth"
    NEAR = "Near"

class NearContract(StrEnum):
    OMNI_BRIDGE = "omni_bridge"
    EVM_PROVER = "evm_prover"
    OMNI_PROVER = "omni_prover"
    TOKEN_DEPLOYER = "token_deployer"
    WORMHOLE_OMNI_PROVER_PROXY = "wormhole_omni_prover_proxy"
    MOCK_TOKEN = "mock_token"


class NearTestAccount(StrEnum):
    INIT_ACCOUNT = "omni_init_account"
    DAO_ACCOUNT = "omni_dao_account"
    RELAYER_ACCOUNT = "omni_relayer_account"
    SENDER_ACCOUNT = "omni_sender_account"


class EvmContract(StrEnum):
    FAKE_PROVER = "fake_prover"
    ENEAR_PROXY = "e_near_proxy"
    TOKEN_IMPL = "token_impl"
    OMNI_BRIDGE = "omni_bridge"
    TEST_TOKEN = "test_token"
    ENEAR = "e_near"
