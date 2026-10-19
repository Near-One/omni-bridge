//! `verify_proof` selects the prover by the relayer-supplied `chain_kind`.
//! These tests check that the proof a prover returns is accepted only when it
//! is about that same chain, so a prover registered for one chain cannot
//! attest events of another chain.

#[cfg(test)]
mod tests {
    use near_sdk::{
        borsh,
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{
        result::{ExecutionFinalResult, ExecutionSuccess},
        types::NearToken,
        Account, Contract,
    };
    use omni_types::{
        locker_args::{
            BindTokenArgs, ClaimFeeArgs, DeployTokenArgs, FinTransferArgs, StorageDepositAction,
        },
        near_events::OmniBridgeEvent,
        prover_result::{
            DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage,
            ProverResult,
        },
        BridgeOnTransferMsg, ChainKind, Fee, InitTransferMsg, OmniAddress, TransferId,
        TransferMessage,
    };
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{
            account_n, base_eoa_address, base_factory_address, base_token_address, build_artifacts,
            eth_eoa_address, eth_factory_address, eth_token_address, get_event_data,
            relayer_account_id, BuildArtifacts, NEP141_DEPOSIT,
        },
    };

    const AMOUNT: u128 = 1000;
    const LOCKED_ON_BASE: u128 = 1_000_000;

    struct TestSetup {
        token_contract: Contract,
        base_token_contract: Contract,
        locker_contract: Contract,
        relayer_account: Account,
        required_balance_for_fin_transfer: NearToken,
    }

    impl TestSetup {
        fn token_contract_for(&self, chain: ChainKind) -> &AccountId {
            match chain {
                ChainKind::Eth => self.token_contract.id(),
                ChainKind::Base => self.base_token_contract.id(),
                _ => panic!("unsupported chain in test"),
            }
        }
    }

    fn factory(chain: ChainKind) -> OmniAddress {
        match chain {
            ChainKind::Eth => eth_factory_address(),
            ChainKind::Base => base_factory_address(),
            _ => panic!("unsupported chain in test"),
        }
    }

    fn token(chain: ChainKind) -> OmniAddress {
        match chain {
            ChainKind::Eth => eth_token_address(),
            ChainKind::Base => base_token_address(),
            _ => panic!("unsupported chain in test"),
        }
    }

    fn sender(chain: ChainKind) -> OmniAddress {
        match chain {
            ChainKind::Eth => eth_eoa_address(),
            ChainKind::Base => base_eoa_address(),
            _ => panic!("unsupported chain in test"),
        }
    }

    /// Bridge with the Eth factory, the Eth prover and the first token bound to
    /// Eth (from the shared environment), plus the Base factory, the same mock
    /// prover registered for Base and a second token bound to Base with a
    /// locked balance.
    #[allow(clippy::too_many_lines)]
    async fn setup(build_artifacts: &BuildArtifacts) -> anyhow::Result<TestSetup> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_native_nep141_token(24)
            .await?;
        let relayer_account = env.setup_trusted_relayer(relayer_account_id()).await?;
        env.storage_deposit(relayer_account.id()).await?;
        let worker = env.worker;
        let locker_contract = env.bridge_contract;
        let token_contract = env.token.contract;

        locker_contract
            .call("add_factory")
            .args_json(json!({ "address": base_factory_address() }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let provers: Vec<(ChainKind, AccountId)> =
            locker_contract.view("get_provers").await?.json()?;
        let (_, eth_prover) = provers
            .into_iter()
            .find(|(chain, _)| *chain == ChainKind::Eth)
            .expect("Eth prover registered by the environment");
        locker_contract
            .call("add_prover")
            .args_json(json!({ "chain": "Base", "account_id": eth_prover }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let base_token_contract = worker.dev_deploy(&build_artifacts.mock_token).await?;
        base_token_contract
            .call("new_default_meta")
            .args_json(json!({
                "owner_id": base_token_contract.id(),
                "total_supply": U128(u128::MAX)
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        base_token_contract
            .call("storage_deposit")
            .args_json(json!({
                "account_id": locker_contract.id(),
                "registration_only": true,
            }))
            .deposit(NEP141_DEPOSIT)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Positive control for bind_token: a Base proof through the Base prover.
        let required_deposit: NearToken = locker_contract
            .view("required_balance_for_bind_token")
            .await?
            .json()?;
        let bind_result = locker_contract
            .call("bind_token")
            .args_borsh(bind_token_args(
                base_token_contract.id(),
                ChainKind::Base,
                ChainKind::Base,
            ))
            .deposit(required_deposit)
            .max_gas()
            .transact()
            .await?;
        assert!(
            bind_result.failures().is_empty(),
            "bind_token failed: {:?}",
            bind_result.failures()
        );
        let bound: Option<OmniAddress> = locker_contract
            .view("get_token_address")
            .args_json(json!({
                "chain_kind": ChainKind::Base,
                "token": base_token_contract.id(),
            }))
            .await?
            .json()?;
        assert_eq!(bound, Some(base_token_address()));

        locker_contract
            .call("set_locked_tokens")
            .args_json(json!({
                "args": [{
                    "chain_kind": ChainKind::Base,
                    "token_id": base_token_contract.id(),
                    "amount": U128(LOCKED_ON_BASE),
                }]
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        base_token_contract
            .call("ft_transfer")
            .args_json(json!({
                "receiver_id": locker_contract.id(),
                "amount": U128(LOCKED_ON_BASE),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let required_balance_for_fin_transfer: NearToken = locker_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        Ok(TestSetup {
            token_contract,
            base_token_contract,
            locker_contract,
            relayer_account,
            required_balance_for_fin_transfer,
        })
    }

    fn bind_token_args(
        token_contract: &AccountId,
        proof_chain: ChainKind,
        chain_kind: ChainKind,
    ) -> BindTokenArgs {
        let prover_result = ProverResult::DeployToken(DeployTokenMessage {
            token: token_contract.clone(),
            token_address: token(proof_chain),
            emitter_address: factory(proof_chain),
            decimals: 24,
            origin_decimals: 24,
        });
        BindTokenArgs {
            chain_kind,
            prover_args: borsh::to_vec(&prover_result).unwrap(),
        }
    }

    fn fin_transfer_args(
        token_contract: &AccountId,
        recipient: &AccountId,
        proof_chain: ChainKind,
        chain_kind: ChainKind,
    ) -> FinTransferArgs {
        let prover_result = ProverResult::InitTransfer(InitTransferMessage {
            origin_nonce: 1,
            token: token(proof_chain),
            recipient: OmniAddress::Near(recipient.clone()),
            amount: U128(AMOUNT),
            fee: Fee {
                fee: U128(0),
                native_fee: U128(0),
            },
            sender: sender(proof_chain),
            msg: String::new(),
            emitter_address: factory(proof_chain),
        });
        FinTransferArgs {
            chain_kind,
            storage_deposit_actions: vec![StorageDepositAction {
                token_id: token_contract.clone(),
                account_id: recipient.clone(),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            }],
            prover_args: borsh::to_vec(&prover_result).unwrap(),
        }
    }

    fn transfer_message_from_event(result: &ExecutionSuccess) -> anyhow::Result<TransferMessage> {
        let logs = result
            .receipt_outcomes()
            .iter()
            .flat_map(|outcome| &outcome.logs)
            .collect::<Vec<_>>();
        let event: OmniBridgeEvent = serde_json::from_value(
            get_event_data("InitTransferEvent", &logs)?
                .ok_or_else(|| anyhow::anyhow!("InitTransferEvent not found"))?,
        )?;
        let OmniBridgeEvent::InitTransferEvent { transfer_message } = event else {
            anyhow::bail!("unexpected event")
        };
        Ok(transfer_message)
    }

    /// `bind_token` and `deploy_token` end in a refund callback, so the
    /// transaction itself succeeds and the panic sits in a receipt.
    fn assert_error(result: &ExecutionFinalResult, expected: &str) {
        let failures = result.failures();
        assert!(!failures.is_empty(), "call must fail");
        let text = format!("{failures:?}");
        assert!(text.contains(expected), "expected {expected}, got: {text}");
    }

    #[rstest]
    #[case(ChainKind::Base, ChainKind::Eth)]
    #[case(ChainKind::Eth, ChainKind::Base)]
    #[tokio::test]
    async fn fin_transfer_proof_chain_differs_from_prover_chain_rejected(
        #[case] proof_chain: ChainKind,
        #[case] chain_kind: ChainKind,
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        let result = setup
            .relayer_account
            .call(setup.locker_contract.id(), "fin_transfer")
            .args_borsh(fin_transfer_args(
                setup.token_contract_for(proof_chain),
                &account_n(1),
                proof_chain,
                chain_kind,
            ))
            .deposit(NEP141_DEPOSIT.saturating_add(setup.required_balance_for_fin_transfer))
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROOF_CHAIN_MISMATCH");

        for token_contract in [&setup.token_contract, &setup.base_token_contract] {
            let recipient_balance: U128 = token_contract
                .view("ft_balance_of")
                .args_json(json!({ "account_id": account_n(1) }))
                .await?
                .json()?;
            assert_eq!(recipient_balance.0, 0);
        }

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn fin_transfer_proof_chain_matches_prover_chain_accepted(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        setup
            .relayer_account
            .call(setup.locker_contract.id(), "fin_transfer")
            .args_borsh(fin_transfer_args(
                setup.base_token_contract.id(),
                &account_n(1),
                ChainKind::Base,
                ChainKind::Base,
            ))
            .deposit(NEP141_DEPOSIT.saturating_add(setup.required_balance_for_fin_transfer))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let recipient_balance: U128 = setup
            .base_token_contract
            .view("ft_balance_of")
            .args_json(json!({ "account_id": account_n(1) }))
            .await?
            .json()?;
        assert_eq!(recipient_balance.0, AMOUNT);

        let locked: Option<U128> = setup
            .locker_contract
            .view("get_locked_tokens")
            .args_json(json!({
                "chain_kind": ChainKind::Base,
                "token_id": setup.base_token_contract.id(),
            }))
            .await?
            .json()?;
        assert_eq!(locked, Some(U128(LOCKED_ON_BASE - AMOUNT)));

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn fin_transfer_without_prover_for_chain_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        let result = setup
            .relayer_account
            .call(setup.locker_contract.id(), "fin_transfer")
            .args_borsh(fin_transfer_args(
                setup.token_contract.id(),
                &account_n(1),
                ChainKind::Eth,
                ChainKind::Arb,
            ))
            .deposit(NEP141_DEPOSIT.saturating_add(setup.required_balance_for_fin_transfer))
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROVER_FOR_CHAIN_KIND_NOT_REGISTERED");

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn claim_fee_proof_chain_differs_from_prover_chain_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        let prover_result = ProverResult::FinTransfer(FinTransferMessage {
            transfer_id: TransferId {
                origin_chain: ChainKind::Near,
                origin_nonce: 1,
            },
            fee_recipient: Some(setup.relayer_account.id().clone()),
            amount: U128(AMOUNT),
            emitter_address: base_factory_address(),
        });
        let result = setup
            .relayer_account
            .call(setup.locker_contract.id(), "claim_fee")
            .args_borsh(ClaimFeeArgs {
                chain_kind: ChainKind::Eth,
                prover_args: borsh::to_vec(&prover_result)?,
            })
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROOF_CHAIN_MISMATCH");

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn deploy_token_proof_chain_differs_from_prover_chain_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        let prover_result = ProverResult::LogMetadata(LogMetadataMessage {
            token_address: base_token_address(),
            name: "Base Token".to_string(),
            symbol: "BT".to_string(),
            decimals: 18,
            emitter_address: base_factory_address(),
        });
        let required_deposit: NearToken = setup
            .locker_contract
            .view("required_balance_for_deploy_token")
            .await?
            .json()?;
        let result = setup
            .locker_contract
            .call("deploy_token")
            .args_borsh(DeployTokenArgs {
                chain_kind: ChainKind::Eth,
                prover_args: borsh::to_vec(&prover_result)?,
            })
            .deposit(required_deposit)
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROOF_CHAIN_MISMATCH");

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn bind_token_proof_chain_differs_from_prover_chain_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        let required_deposit: NearToken = setup
            .locker_contract
            .view("required_balance_for_bind_token")
            .await?
            .json()?;
        let result = setup
            .locker_contract
            .call("bind_token")
            .args_borsh(bind_token_args(
                setup.token_contract.id(),
                ChainKind::Base,
                ChainKind::Eth,
            ))
            .deposit(required_deposit)
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROOF_CHAIN_MISMATCH");

        let bound: Option<OmniAddress> = setup
            .locker_contract
            .view("get_token_address")
            .args_json(json!({
                "chain_kind": ChainKind::Base,
                "token": setup.token_contract.id(),
            }))
            .await?
            .json()?;
        assert_eq!(bound, None);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn claim_fee_prover_chain_differs_from_transfer_destination_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let setup = setup(build_artifacts).await?;

        // Create a NEAR -> Eth transfer.
        let sender_account = setup.relayer_account.clone();
        setup
            .token_contract
            .call("ft_transfer")
            .args_json(json!({
                "receiver_id": sender_account.id(),
                "amount": U128(AMOUNT),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        let init_transfer_msg = InitTransferMsg {
            recipient: eth_eoa_address(),
            fee: U128(0),
            native_token_fee: U128(0),
            msg: None,
            external_id: None,
        };
        let storage_deposit: NearToken = setup
            .locker_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({
                "recipient": init_transfer_msg.recipient,
                "sender": OmniAddress::Near(sender_account.id().clone()),
            }))
            .await?
            .json()?;
        let account_balance: NearToken = setup
            .locker_contract
            .view("required_balance_for_account")
            .await?
            .json()?;
        sender_account
            .call(setup.locker_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": sender_account.id() }))
            .deposit(storage_deposit.saturating_add(account_balance))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        let transfer_result = sender_account
            .call(setup.token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": setup.locker_contract.id(),
                "amount": U128(AMOUNT),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_transfer_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        let transfer_message = transfer_message_from_event(&transfer_result)?;
        assert_eq!(transfer_message.recipient.get_chain(), ChainKind::Eth);

        // A FinTransfer proof from the Base factory, through the Base prover,
        // claiming the Eth-bound transfer.
        let prover_result = ProverResult::FinTransfer(FinTransferMessage {
            transfer_id: TransferId {
                origin_chain: ChainKind::Near,
                origin_nonce: transfer_message.origin_nonce,
            },
            fee_recipient: Some(setup.relayer_account.id().clone()),
            amount: U128(AMOUNT),
            emitter_address: base_factory_address(),
        });
        let result = setup
            .relayer_account
            .call(setup.locker_contract.id(), "claim_fee")
            .args_borsh(ClaimFeeArgs {
                chain_kind: ChainKind::Base,
                prover_args: borsh::to_vec(&prover_result)?,
            })
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?;
        assert_error(&result, "ERR_PROOF_CHAIN_MISMATCH");

        // The transfer is still pending.
        let pending: serde_json::Value = setup
            .locker_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: transfer_message.origin_nonce,
                },
            }))
            .await?
            .json()?;
        assert!(pending.is_object());

        Ok(())
    }
}
