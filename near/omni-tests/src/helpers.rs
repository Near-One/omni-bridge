#[cfg(test)]
pub mod tests {
    use std::{path::Path, str::FromStr};

    use near_sdk::{borsh, json_types::U128, serde_json, AccountId};
    use near_workspaces::types::NearToken;
    use omni_types::{
        locker_args::{BindTokenArgs, ClaimFeeArgs, DeployTokenArgs},
        prover_result::{DeployTokenMessage, FinTransferMessage, LogMetadataMessage, ProverResult},
        BasicMetadata, ChainKind, Nonce, OmniAddress, TransferId,
    };
    use rstest::fixture;

    pub const NEP141_DEPOSIT: NearToken = NearToken::from_yoctonear(1_250_000_000_000_000_000_000);

    fn build_wasm(path: &str, target_dir: &str) -> Vec<u8> {
        let pwd = Path::new("./").canonicalize().expect("new path");
        let sub_target = pwd.join(format!("target/{target_dir}"));

        let artifact = cargo_near_build::build(cargo_near_build::BuildOpts {
            manifest_path: Some(
                cargo_near_build::camino::Utf8PathBuf::from_str(path)
                    .expect("camino PathBuf from str"),
            ),
            override_cargo_target_dir: Some(sub_target.to_string_lossy().to_string()),
            ..Default::default()
        })
        .unwrap_or_else(|_| panic!("building contract from {path}"));

        std::fs::read(&artifact.path).unwrap()
    }

    #[fixture]
    pub fn mock_token_wasm() -> Vec<u8> {
        build_wasm(
            "../mock/mock-token/Cargo.toml",
            "test-target-for-mock-token",
        )
    }

    #[fixture]
    pub fn mock_prover_wasm() -> Vec<u8> {
        build_wasm(
            "../mock/mock-prover/Cargo.toml",
            "test-target-for-mock-prover",
        )
    }

    #[fixture]
    pub fn locker_wasm() -> Vec<u8> {
        build_wasm("../omni-bridge/Cargo.toml", "test-target-for-locker")
    }

    #[fixture]
    pub fn token_deployer_wasm() -> Vec<u8> {
        build_wasm(
            "../token-deployer/Cargo.toml",
            "test-target-for-token-deployer",
        )
    }

    pub fn relayer_account_id() -> AccountId {
        "relayer".parse().unwrap()
    }

    pub fn account_n(n: u8) -> AccountId {
        format!("account_{n}").parse().unwrap()
    }

    pub fn eth_factory_address() -> OmniAddress {
        "eth:0x252e87862A3A720287E7fd527cE6e8d0738427A2"
            .parse()
            .unwrap()
    }

    pub fn arb_factory_address() -> OmniAddress {
        "arb:0x252e87862A3A720287E7fd527cE6e8d0738427A2"
            .parse()
            .unwrap()
    }

    pub fn base_factory_address() -> OmniAddress {
        "base:0x252e87862A3A720287E7fd527cE6e8d0738427A2"
            .parse()
            .unwrap()
    }

    pub fn sol_factory_address() -> OmniAddress {
        "sol:11111111111111111111111111111111".parse().unwrap()
    }

    pub fn eth_eoa_address() -> OmniAddress {
        "eth:0xc5ed912ca6db7b41de4ef3632fa0a5641e42bf09"
            .parse()
            .unwrap()
    }

    pub fn eth_token_address() -> OmniAddress {
        "eth:0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap()
    }

    pub fn sol_token_address() -> OmniAddress {
        "sol:11111111111111111111111111111111".parse().unwrap()
    }

    pub fn arb_token_address() -> OmniAddress {
        "arb:0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap()
    }

    pub fn base_token_address() -> OmniAddress {
        "base:0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap()
    }

    pub fn get_claim_fee_args_near(
        origin_chain: ChainKind,
        destination_chain: ChainKind,
        origin_nonce: Nonce,
        fee_recipient: &AccountId,
        amount: u128,
        emitter_address: OmniAddress,
    ) -> ClaimFeeArgs {
        let fin_transfer = FinTransferMessage {
            transfer_id: TransferId {
                origin_chain,
                origin_nonce,
            },
            fee_recipient: fee_recipient.clone(),
            amount: U128(amount),
            emitter_address,
        };

        let prover_result = ProverResult::FinTransfer(fin_transfer);

        let prover_args = borsh::to_vec(&prover_result).expect("Failed to serialize prover result");

        ClaimFeeArgs {
            chain_kind: destination_chain,
            prover_args,
        }
    }

    pub fn get_test_deploy_token_args(
        token_address: &OmniAddress,
        factory_contract_address: &OmniAddress,
        token_metadata: &BasicMetadata,
    ) -> DeployTokenArgs {
        let log_metadata_message = LogMetadataMessage {
            token_address: token_address.clone(),
            name: token_metadata.name.clone(),
            symbol: token_metadata.symbol.clone(),
            decimals: token_metadata.decimals,
            emitter_address: factory_contract_address.clone(),
        };

        let prover_result = ProverResult::LogMetadata(log_metadata_message);
        let prover_args = borsh::to_vec(&prover_result).expect("Failed to serialize prover result");

        DeployTokenArgs {
            chain_kind: token_address.get_chain(),
            prover_args,
        }
    }

    pub fn get_bind_token_args(
        token: &AccountId,
        token_address: &OmniAddress,
        emitter_address: &OmniAddress,
        decimals: u8,
        origin_decimals: u8,
    ) -> BindTokenArgs {
        let deploy_token_message = DeployTokenMessage {
            token: token.clone(),
            token_address: token_address.clone(),
            emitter_address: emitter_address.clone(),
            decimals,
            origin_decimals,
        };
        let prover_result = ProverResult::DeployToken(deploy_token_message);

        let prover_args = borsh::to_vec(&prover_result).expect("Failed to serialize prover result");

        BindTokenArgs {
            chain_kind: ChainKind::Eth,
            prover_args,
        }
    }

    pub fn get_event_data(
        event_name: &str,
        logs: &Vec<&String>,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        logs.iter()
            .find(|log| log.contains(event_name))
            .map(|log| serde_json::from_str(log).map_err(anyhow::Error::from))
            .transpose()
    }
}
