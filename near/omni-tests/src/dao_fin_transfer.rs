#[cfg(test)]
mod tests {
    use near_sdk::{
        borsh,
        json_types::U128,
        serde_json::{self, json},
        AccountId,
    };
    use near_workspaces::{types::NearToken, Account, Contract};
    use omni_types::{
        locker_args::{FinTransferArgs, StorageDepositAction},
        prover_result::{InitTransferMessage, ProverResult},
        BridgeOnTransferMsg, ChainKind, FastFinTransferMsg, Fee, OmniAddress, TransferId,
        TransferIdKind, TransferMessage, UnifiedTransferId,
    };
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{
            account_n, base_eoa_address, build_artifacts, eth_eoa_address, eth_factory_address,
            eth_token_address, relayer_account_id, BuildArtifacts, NEP141_DEPOSIT,
        },
    };

    struct TestSetup {
        token_contract: Contract,
        bridge_contract: Contract,
        dao_account: Account,
        relayer_account: Account,
        required_balance_for_fin_transfer: NearToken,
    }

    async fn setup_contracts(build_artifacts: &BuildArtifacts) -> anyhow::Result<TestSetup> {
        let env_builder = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_native_nep141_token(24)
            .await?;

        let relayer_account = env_builder
            .setup_trusted_relayer(relayer_account_id())
            .await?;

        // The bridge account is ACL super admin (granted in `new`), so it can
        // grant the DAO role to a dedicated account, mirroring production.
        let dao_account = env_builder.create_account(account_n(2)).await?;
        env_builder
            .bridge_contract
            .call("acl_grant_role")
            .args_json(json!({
                "role": "DAO",
                "account_id": dao_account.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let required_balance_for_fin_transfer: NearToken = env_builder
            .bridge_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;

        Ok(TestSetup {
            token_contract: env_builder.token.contract,
            bridge_contract: env_builder.bridge_contract,
            dao_account,
            relayer_account,
            required_balance_for_fin_transfer,
        })
    }

    fn init_transfer_message(
        recipient: OmniAddress,
        amount: u128,
        fee: u128,
    ) -> InitTransferMessage {
        InitTransferMessage {
            origin_nonce: 1,
            token: eth_token_address(),
            amount: U128(amount),
            recipient,
            fee: Fee {
                fee: U128(fee),
                native_fee: U128(0),
            },
            sender: eth_eoa_address(),
            msg: String::new(),
            emitter_address: eth_factory_address(),
        }
    }

    async fn fund_bridge(
        token_contract: &Contract,
        bridge_id: &AccountId,
        amount: u128,
    ) -> anyhow::Result<()> {
        token_contract
            .call("ft_transfer")
            .args_json(json!({
                "receiver_id": bridge_id,
                "amount": U128(amount),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        Ok(())
    }

    async fn ft_balance_of(
        token_contract: &Contract,
        account_id: &AccountId,
    ) -> anyhow::Result<u128> {
        let balance: U128 = token_contract
            .view("ft_balance_of")
            .args_json(json!({ "account_id": account_id }))
            .await?
            .json()?;
        Ok(balance.0)
    }

    async fn is_transfer_finalised(
        bridge_contract: &Contract,
        origin_nonce: u64,
    ) -> anyhow::Result<bool> {
        Ok(bridge_contract
            .view("is_transfer_finalised")
            .args_json(json!({
                "transfer_id": { "origin_chain": "Eth", "origin_nonce": origin_nonce },
            }))
            .await?
            .json()?)
    }

    /// Typed args for `fin_transfer_as_dao`. The `json!` macro goes through
    /// `serde_json::Value`, which cannot represent u128 values above
    /// `u64::MAX` (e.g. `storage_deposit_amount`) — the writer/reader paths
    /// used by `args_json` and the contract handle the full u128 range.
    #[derive(near_sdk::serde::Serialize, near_sdk::serde::Deserialize)]
    #[serde(crate = "near_sdk::serde")]
    struct FinTransferAsDaoArgs {
        init_transfer: InitTransferMessage,
        storage_deposit_actions: Vec<StorageDepositAction>,
    }

    #[test]
    fn dao_args_serialize_full_u128_range() {
        let deposit = NEP141_DEPOSIT.as_yoctonear();
        assert!(deposit > u128::from(u64::MAX));

        let args = FinTransferAsDaoArgs {
            init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), 1000, 1),
            storage_deposit_actions: vec![StorageDepositAction {
                token_id: account_n(3),
                account_id: account_n(1),
                storage_deposit_amount: Some(deposit),
            }],
        };

        let bytes = serde_json::to_vec(&args).expect("client-side serialization");
        let decoded: FinTransferAsDaoArgs =
            serde_json::from_slice(&bytes).expect("contract-side deserialization");
        assert_eq!(
            Some(deposit),
            decoded.storage_deposit_actions[0].storage_deposit_amount
        );
    }

    /// Calls the proof-based `fin_transfer` (the mock prover echoes `prover_args`).
    async fn fin_transfer_with_proof(
        bridge_contract: &Contract,
        relayer_account: &Account,
        init_transfer: InitTransferMessage,
        storage_deposit_actions: Vec<StorageDepositAction>,
        deposit: NearToken,
    ) -> anyhow::Result<near_workspaces::result::ExecutionFinalResult> {
        Ok(relayer_account
            .call(bridge_contract.id(), "fin_transfer")
            .args_borsh(FinTransferArgs {
                chain_kind: ChainKind::Eth,
                storage_deposit_actions,
                prover_args: borsh::to_vec(&ProverResult::InitTransfer(init_transfer))?,
            })
            .deposit(deposit)
            .max_gas()
            .transact()
            .await?)
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_success(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            token_contract,
            bridge_contract,
            dao_account,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(build_artifacts).await?;
        let (amount, fee) = (1000, 1);

        fund_bridge(&token_contract, bridge_contract.id(), amount).await?;

        let storage_deposit_actions = vec![
            StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: account_n(1),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            },
            StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: dao_account.id().clone(),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            },
        ];
        let attached_deposit = NEP141_DEPOSIT
            .saturating_mul(2)
            .saturating_add(required_balance_for_fin_transfer);

        dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), amount, fee),
                storage_deposit_actions,
            })
            .deposit(attached_deposit)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert_eq!(
            amount - fee,
            ft_balance_of(&token_contract, &account_n(1)).await?
        );
        assert_eq!(fee, ft_balance_of(&token_contract, dao_account.id()).await?);
        assert_eq!(
            0,
            ft_balance_of(&token_contract, bridge_contract.id()).await?
        );

        assert!(is_transfer_finalised(&bridge_contract, 1).await?);

        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_non_dao_caller_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            bridge_contract,
            relayer_account,
            ..
        } = setup_contracts(build_artifacts).await?;

        let err = relayer_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), 1000, 0),
                storage_deposit_actions: Vec::new(),
            })
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("non-DAO caller must be rejected")
            .to_string();

        assert!(
            err.contains("Insufficient permissions"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    async fn get_locked_tokens(
        bridge_contract: &Contract,
        chain_kind: ChainKind,
        token_id: &AccountId,
    ) -> anyhow::Result<u128> {
        let locked_tokens: Option<U128> = bridge_contract
            .view("get_locked_tokens")
            .args_json(json!({
                "chain_kind": chain_kind,
                "token_id": token_id,
            }))
            .await?
            .json()?;
        Ok(locked_tokens.unwrap_or(U128(0)).0)
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_to_other_chain_creates_pending_transfer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            token_contract,
            bridge_contract,
            dao_account,
            required_balance_for_fin_transfer,
            ..
        } = setup_contracts(build_artifacts).await?;
        let (amount, fee) = (1000, 1);

        let locked_eth_before =
            get_locked_tokens(&bridge_contract, ChainKind::Eth, token_contract.id()).await?;
        let locked_base_before =
            get_locked_tokens(&bridge_contract, ChainKind::Base, token_contract.id()).await?;

        let required_balance_for_init_transfer: NearToken = bridge_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({ "msg": None::<String> }))
            .await?
            .json()?;

        // No token payout on NEAR happens for a routed transfer, so no
        // storage deposit actions are needed.
        dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(base_eoa_address(), amount, fee),
                storage_deposit_actions: Vec::new(),
            })
            .deposit(
                required_balance_for_fin_transfer
                    .saturating_add(required_balance_for_init_transfer),
            )
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        assert!(is_transfer_finalised(&bridge_contract, 1).await?);

        // The pending transfer message awaits the usual sign_transfer flow
        let transfer_message: TransferMessage = bridge_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Eth,
                    origin_nonce: 1,
                },
            }))
            .await?
            .json()?;
        assert_eq!(base_eoa_address(), transfer_message.recipient);
        assert_eq!(amount, transfer_message.amount.0);
        assert_eq!(fee, transfer_message.fee.fee.0);

        // Locked-token lanes moved: origin unlocked, destination locked
        assert_eq!(
            locked_eth_before - amount,
            get_locked_tokens(&bridge_contract, ChainKind::Eth, token_contract.id()).await?
        );
        assert_eq!(
            locked_base_before + amount,
            get_locked_tokens(&bridge_contract, ChainKind::Base, token_contract.id()).await?
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_chain_mismatch_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            bridge_contract,
            dao_account,
            ..
        } = setup_contracts(build_artifacts).await?;

        // Sender on Base while token and emitter are on Eth
        let mut init_transfer = init_transfer_message(OmniAddress::Near(account_n(1)), 1000, 0);
        init_transfer.sender = base_eoa_address();

        let err = dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer,
                storage_deposit_actions: Vec::new(),
            })
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("chain mismatch must be rejected")
            .to_string();

        assert!(
            err.contains("ERR_CANNOT_DETERMINE_ORIGIN_CHAIN"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_unknown_emitter_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            bridge_contract,
            dao_account,
            ..
        } = setup_contracts(build_artifacts).await?;

        // Consistent chains, but the emitter is not the registered Eth factory
        let mut init_transfer = init_transfer_message(OmniAddress::Near(account_n(1)), 1000, 0);
        init_transfer.emitter_address = eth_eoa_address();

        let err = dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer,
                storage_deposit_actions: Vec::new(),
            })
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("unknown emitter must be rejected")
            .to_string();

        assert!(
            err.contains("ERR_UNKNOWN_FACTORY"),
            "unexpected error: {err}"
        );

        assert!(
            !is_transfer_finalised(&bridge_contract, 1).await?,
            "failed finalization must not consume the transfer id"
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_missing_storage_actions_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            bridge_contract,
            dao_account,
            ..
        } = setup_contracts(build_artifacts).await?;

        // A valid transfer to a NEAR recipient but WITHOUT the required
        // storage deposit action for the recipient: must fail with the
        // dedicated error (short-circuit in process_fin_transfer_to_near),
        // not an out-of-bounds panic, and must not consume the transfer id.
        let err = dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), 1000, 0),
                storage_deposit_actions: Vec::new(),
            })
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("missing recipient storage action must be rejected")
            .to_string();

        assert!(
            err.contains("ERR_STORAGE_RECIPIENT_OMITTED"),
            "unexpected error: {err}"
        );

        assert!(
            !is_transfer_finalised(&bridge_contract, 1).await?,
            "failed finalization must not consume the transfer id"
        );
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_proof_after_dao_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            token_contract,
            bridge_contract,
            dao_account,
            relayer_account,
            required_balance_for_fin_transfer,
        } = setup_contracts(build_artifacts).await?;
        let amount = 1000;

        fund_bridge(&token_contract, bridge_contract.id(), amount).await?;

        // 1) DAO finalizes without proof
        dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), amount, 0),
                storage_deposit_actions: vec![StorageDepositAction {
                    token_id: token_contract.id().clone(),
                    account_id: account_n(1),
                    storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
                }],
            })
            .deposit(NEP141_DEPOSIT.saturating_add(required_balance_for_fin_transfer))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // 2) A proof for the same transfer id arrives later — must be rejected
        let err = fin_transfer_with_proof(
            &bridge_contract,
            &relayer_account,
            init_transfer_message(OmniAddress::Near(account_n(1)), amount, 0),
            vec![StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: account_n(1),
                storage_deposit_amount: None,
            }],
            required_balance_for_fin_transfer,
        )
        .await?
        .into_result()
        .expect_err("replay with proof must be rejected")
        .to_string();

        assert!(
            err.contains("ERR_TRANSFER_ALREADY_FINALISED"),
            "unexpected error: {err}"
        );

        // Recipient was paid exactly once
        assert_eq!(amount, ft_balance_of(&token_contract, &account_n(1)).await?);
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_after_proof_rejected(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let TestSetup {
            token_contract,
            bridge_contract,
            dao_account,
            relayer_account,
            required_balance_for_fin_transfer,
        } = setup_contracts(build_artifacts).await?;
        let amount = 1000;

        fund_bridge(&token_contract, bridge_contract.id(), amount).await?;

        // 1) Normal proof-based finalization
        fin_transfer_with_proof(
            &bridge_contract,
            &relayer_account,
            init_transfer_message(OmniAddress::Near(account_n(1)), amount, 0),
            vec![StorageDepositAction {
                token_id: token_contract.id().clone(),
                account_id: account_n(1),
                storage_deposit_amount: Some(NEP141_DEPOSIT.as_yoctonear()),
            }],
            NEP141_DEPOSIT.saturating_add(required_balance_for_fin_transfer),
        )
        .await?
        .into_result()
        .expect("proof-based finalization must succeed");

        // 2) DAO tries to finalize the same transfer id — must be rejected
        let err = dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), amount, 0),
                storage_deposit_actions: vec![StorageDepositAction {
                    token_id: token_contract.id().clone(),
                    account_id: account_n(1),
                    storage_deposit_amount: None,
                }],
            })
            .deposit(required_balance_for_fin_transfer)
            .max_gas()
            .transact()
            .await?
            .into_result()
            .expect_err("DAO replay must be rejected")
            .to_string();

        assert!(
            err.contains("ERR_TRANSFER_ALREADY_FINALISED"),
            "unexpected error: {err}"
        );

        assert_eq!(amount, ft_balance_of(&token_contract, &account_n(1)).await?);
        Ok(())
    }

    #[rstest]
    #[tokio::test]
    async fn test_fin_transfer_as_dao_after_fast_transfer_pays_relayer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env_builder = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_native_nep141_token(24)
            .await?;

        let relayer_account = env_builder
            .setup_trusted_relayer(relayer_account_id())
            .await?;
        env_builder.storage_deposit(relayer_account.id()).await?;
        env_builder
            .mint_tokens(relayer_account.id(), 1_000_000)
            .await?;

        let dao_account = env_builder.create_account(account_n(2)).await?;
        env_builder
            .bridge_contract
            .call("acl_grant_role")
            .args_json(json!({
                "role": "DAO",
                "account_id": dao_account.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let (amount, fee) = (1000, 1);
        let token_contract = &env_builder.token.contract;
        let bridge_contract = &env_builder.bridge_contract;

        // The bridge needs tokens to pay out the later finalization
        fund_bridge(token_contract, bridge_contract.id(), amount).await?;

        // Relayer needs bridge-side storage balance for the fast transfer
        let required_balance_for_account: NearToken = bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;
        let required_balance_for_fast_transfer: NearToken = bridge_contract
            .view("required_balance_for_fast_transfer")
            .await?
            .json()?;
        relayer_account
            .call(bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": relayer_account.id() }))
            .deposit(
                required_balance_for_account
                    .saturating_add(required_balance_for_fast_transfer)
                    .saturating_add(NEP141_DEPOSIT),
            )
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // 1) Relayer fronts the transfer (fast transfer to NEAR)
        let fast_transfer_msg = FastFinTransferMsg {
            transfer_id: UnifiedTransferId {
                origin_chain: ChainKind::Eth,
                kind: TransferIdKind::Nonce(1),
            },
            recipient: OmniAddress::Near(account_n(1)),
            fee: Fee {
                fee: U128(fee),
                native_fee: U128(0),
            },
            msg: String::new(),
            amount: U128(amount),
            storage_deposit_amount: Some(U128(NEP141_DEPOSIT.as_yoctonear())),
            relayer: relayer_account.id().clone(),
        };
        relayer_account
            .call(token_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": bridge_contract.id(),
                "amount": U128(amount - fee),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::FastFinTransfer(
                    fast_transfer_msg
                ))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Recipient was already paid by the relayer
        assert_eq!(
            amount - fee,
            ft_balance_of(token_contract, &account_n(1)).await?
        );
        let relayer_balance_after_fast =
            ft_balance_of(token_contract, relayer_account.id()).await?;

        // 2) DAO finalizes the same transfer — payout must go to the fronting relayer
        let required_balance_for_fin_transfer: NearToken = bridge_contract
            .view("required_balance_for_fin_transfer")
            .await?
            .json()?;
        let relayer_action = StorageDepositAction {
            token_id: token_contract.id().clone(),
            account_id: relayer_account.id().clone(),
            storage_deposit_amount: None,
        };
        dao_account
            .call(bridge_contract.id(), "fin_transfer_as_dao")
            .args_json(FinTransferAsDaoArgs {
                init_transfer: init_transfer_message(OmniAddress::Near(account_n(1)), amount, fee),
                storage_deposit_actions: vec![relayer_action.clone(), relayer_action],
            })
            .deposit(required_balance_for_fin_transfer)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Relayer got principal without fee + fee = full amount back
        assert_eq!(
            relayer_balance_after_fast + amount,
            ft_balance_of(token_contract, relayer_account.id()).await?
        );
        // Recipient did NOT get paid twice
        assert_eq!(
            amount - fee,
            ft_balance_of(token_contract, &account_n(1)).await?
        );
        Ok(())
    }
}
