from enum import StrEnum
from pathlib import Path
from datetime import datetime

# Common
common_testing_root = Path(__file__).parent.parent
common_generated_dir = common_testing_root / "generated"
common_tools_dir = common_testing_root / "tools"
common_scripts_dir = common_tools_dir / "src" / "scripts"
common_tools_compile_stamp = common_generated_dir / "common_tools_compile.stamp"
common_timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
common_bridge_sdk_config_file = common_testing_root / "bridge-sdk-config.json"

# Near
near_deploy_results_dir = common_generated_dir / "near_deploy_results"
near_account_dir = common_generated_dir / "near_accounts"
near_binary_dir = common_generated_dir / "near_artifacts"
near_init_params_file = common_testing_root / "near_init_params.json"

# EVM
evm_deploy_results_dir = common_generated_dir / "evm_deploy_results"


def get_evm_deploy_results_dir(network):
    return f"{evm_deploy_results_dir}/{network}"


def get_evm_account_dir(network):
    return evm_deploy_results_dir / network / "accounts"


class Chain(StrEnum):
    ETH = "Eth"
    NEAR = "Near"
    SOL = "Sol"
    BASE = "Base"
    ARB = "Arb"
    BTC = "Btc"

    @classmethod
    def from_evm_network(cls, evm_network):
        if evm_network == EvmNetwork.ETH_SEPOLIA:
            return cls.ETH
        elif evm_network == EvmNetwork.ARBITRUM_SEPOLIA:
            return cls.ARB
        elif evm_network == EvmNetwork.BASE_SEPOLIA:
            return cls.BASE
        else:
            raise ValueError(f"Unknown EVM network: {evm_network}")


class NearContract(StrEnum):
    OMNI_BRIDGE = "omni_bridge"
    EVM_PROVER = "evm_prover"
    TOKEN_DEPLOYER = "token_deployer"
    WORMHOLE_OMNI_PROVER_PROXY = "wormhole_omni_prover_proxy"
    MOCK_TOKEN = "mock_token"


class NearExternalContract(StrEnum):
    ZCASH_CONNECTOR = "zcash_connector"
    ZCASH_TOKEN = "zcash"
    BTC_TOKEN = "nbtc"
    BTC_CONNECTOR = "btc_connector"


class NearTestAccount(StrEnum):
    INIT_ACCOUNT = "omni_init_account"
    DAO_ACCOUNT = "omni_dao_account"
    RELAYER_ACCOUNT = "omni_relayer_account"
    SENDER_ACCOUNT = "omni_sender_account"
    USER_ACCOUNT = "omni_user_account"
    RECIPIENT_ACCOUNT = "omni_recipient_account"


class EvmContract(StrEnum):
    FAKE_PROVER = "fake_prover"
    ENEAR_PROXY = "e_near_proxy"
    TOKEN_IMPL = "token_impl"
    OMNI_BRIDGE = "omni_bridge"
    TEST_TOKEN = "test_token"
    ENEAR = "e_near"
    USER_ACCOUNT = "user_account"


class EvmNetwork(StrEnum):
    ETH_SEPOLIA = "sepolia"
    ARBITRUM_SEPOLIA = "arbitrumSepolia"
    BASE_SEPOLIA = "baseSepolia"
