#[cfg(test)]
mod tests {
    use near_sdk::{
        json_types::U128,
        serde_json::{self, json},
    };
    use near_workspaces::{types::NearToken, AccountId};
    use omni_types::{
        BridgeOnTransferMsg, ChainKind, Fee, InitTransferMsg, OmniAddress, TransferId,
        TransferMessage, TransferMessageStorageAccount,
    };
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{account_n, build_artifacts, BuildArtifacts, NEP141_DEPOSIT},
    };

    /// Regression test for the stale-pending-transfer bug.
    ///
    /// Pre-fix behavior (the bug):
    /// 1. `init_transfer_resume`'s pre-check used `required_balance_for_init_transfer(msg)`,
    ///    a synthetic estimator that substituted NEAR-account placeholders for
    ///    the recipient/sender/token. For a long Zcash recipient the estimate
    ///    undershot the real cost.
    /// 2. `init_transfer_internal::add_transfer_message` inserted the pending
    ///    entry and measured the real cost via `env::storage_usage()`.
    /// 3. `try_update_storage_balance` then failed (real cost > placeholder
    ///    estimate previously credited to the storage owner), and the function
    ///    returned `transfer_message.amount` as a NEP-141 refund — but the
    ///    inserted entry was NOT removed.
    /// 4. The attacker kept their nzec AND was left with a stale pending entry
    ///    that could be drained via `submit_transfer_to_utxo_chain_connector`.
    ///
    /// Fix (this test verifies):
    /// - `init_transfer_resume` now uses `required_balance_for_init_transfer_message(transfer_message)`,
    ///   the accurate estimator. The attacker depositing only the synthetic
    ///   amount on the message-storage account no longer satisfies the
    ///   pre-check, so `init_transfer_resume` returns the refund before ever
    ///   calling `init_transfer_internal`.
    /// - As defense-in-depth, `init_transfer_internal` also removes the
    ///   pending entry from `pending_transfers` before returning when the
    ///   storage check fails, so any future bug that lets execution reach
    ///   `add_transfer_message` followed by a `try_update_storage_balance`
    ///   failure cannot leave a stale entry behind.
    ///
    /// Either fix alone is sufficient. Together: refund still happens, no
    /// stale `pending_transfers` entry, and `get_transfer_message` returns
    /// `BridgeError::TransferNotExist` for the would-be stale transfer id.
    #[rstest]
    #[tokio::test]
    async fn test_zcash_long_unified_address_does_not_leave_stale_pending_transfer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        // Reuse the existing BTC-flavored setup to get a deployed bridge, mock
        // token, and mock utxo connector. We then attach a Zcash chain config
        // on top (different mock token + different mock connector instance) so
        // the bridge accepts Zcash as a destination chain.
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_utxo_token()
            .await?;

        // Deploy a second NEP-141 ("nzec") to act as the Zcash chain's native
        // wrapper. Same wasm as the BTC-side token.
        let nzec_token = env
            .worker
            .dev_deploy(&build_artifacts.mock_token)
            .await?;
        nzec_token
            .call("new_default_meta")
            .args_json(json!({
                "owner_id": nzec_token.id(),
                "total_supply": U128(u128::MAX),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Deploy a second mock UTXO connector to play the role of
        // zcash-connector.bridge.near.
        let zcash_connector = env
            .worker
            .dev_deploy(&build_artifacts.mock_utxo_connector)
            .await?;
        zcash_connector
            .call("new")
            .args_json(json!({
                "bridge_account": env.bridge_contract.id(),
                "token_account": nzec_token.id(),
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        env.bridge_contract
            .call("add_utxo_chain_connector")
            .args_json(json!({
                "chain_kind": ChainKind::Zcash,
                "utxo_chain_connector_id": zcash_connector.id(),
                "utxo_chain_token_id": nzec_token.id(),
                "decimals": 8,
            }))
            .deposit(NEP141_DEPOSIT.saturating_mul(3))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Give the bridge & connector ft storage on nzec.
        for who in [env.bridge_contract.id(), zcash_connector.id()] {
            nzec_token
                .call("storage_deposit")
                .args_json(json!({
                    "account_id": who,
                    "registration_only": true,
                }))
                .deposit(NEP141_DEPOSIT)
                .max_gas()
                .transact()
                .await?
                .into_result()?;
        }

        // ── Set up the attacker ────────────────────────────────────────────
        //
        // NEAR sandbox reserves 64-char hex IDs for true implicit-account
        // creation (via key transfer, not TLA registration), so we use a
        // shorter named account and compensate with a longer Zcash UA below.
        let attacker_id: AccountId = "attacker".parse()?;
        let attacker = env.create_account(attacker_id.clone()).await?;

        // Register attacker on the bridge with the bare-minimum storage.
        // Not enough to fund a transfer, so init_transfer MUST yield.
        let required_balance_account: NearToken = env
            .bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;
        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": attacker.id() }))
            .deposit(required_balance_account)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // The yield branch charges the bridge contract's own storage balance
        // for the init_transfer_promises entry.
        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": env.bridge_contract.id() }))
            .deposit(NearToken::from_millinear(100))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Give the attacker some nzec to bridge. They will get this back in
        // the refund — but the stale pending transfer survives, enabling the
        // drain.
        let attacker_initial_balance: u128 = 100_000;
        nzec_token
            .call("storage_deposit")
            .args_json(json!({
                "account_id": attacker.id(),
                "registration_only": true,
            }))
            .deposit(NEP141_DEPOSIT)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // The mock token's owner is the token itself; transfer tokens from
        // owner → attacker.
        env.worker
            .root_account()?
            .transfer_near(nzec_token.id(), NearToken::from_near(1))
            .await?
            .into_result()?;
        nzec_token
            .call("ft_transfer")
            .args_json(json!({
                "receiver_id": attacker.id(),
                "amount": U128(attacker_initial_balance),
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // ── Construct the malicious init_transfer ──────────────────────────
        //
        // A 500-char "Zcash UA" string. The bridge doesn't validate Zcash
        // address format — `OmniAddress::Zcash(String)` accepts anything —
        // so we use a string of sufficient length to push the actual encoded
        // size of TransferMessage well beyond the synthetic placeholder.
        //
        // Breakeven (short attacker, dev-deployed token names): ~300 chars.
        // Real-world Zcash UAs with all three receivers (transparent +
        // Sapling + Orchard) are typically 280-320 chars, with longer
        // variants possible. We use 500 to make the overshoot unambiguous
        // and safe across token-name lengths produced by dev-deploy.
        let long_zcash_ua = format!("u{}", "1".repeat(499));
        let recipient = OmniAddress::Zcash(long_zcash_ua.clone());

        let transfer_amount: u128 = 50_000;
        let init_msg = InitTransferMsg {
            recipient: recipient.clone(),
            fee: U128(0),
            native_token_fee: U128(0),
            msg: None,
            external_id: None,
        };

        // Predict the message-storage virtual account. The nonces aren't part
        // of the hash, so we can compute this BEFORE firing the call.
        let storage_account_data = TransferMessageStorageAccount {
            token: OmniAddress::Near(nzec_token.id().clone()),
            amount: U128(transfer_amount),
            recipient: recipient.clone(),
            fee: Fee::default(),
            sender: OmniAddress::Near(attacker.id().clone()),
            msg: String::new(),
        };
        let virtual_acct_id = storage_account_data.id(None);

        // Read the SYNTHETIC placeholder estimate — this is what
        // init_transfer_resume will check the storage payer against.
        let placeholder_estimate: NearToken = env
            .bridge_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({}))
            .await?
            .json()?;

        // ── Fire the malicious ft_transfer_call (it will yield) ────────────
        let status = attacker
            .call(nzec_token.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.bridge_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact_async()
            .await?;

        env.worker.fast_forward(5).await?;

        // ── Resume the yield by depositing EXACTLY the placeholder ─────────
        //
        // Pre-fix: this amount sufficed for the synthetic pre-check, so
        // `init_transfer_resume` called into `init_transfer_internal`, which
        // then failed its real-cost storage check after the insert — leaving
        // a stale entry.
        //
        // Post-fix: `init_transfer_resume` now uses the accurate estimator,
        // so this placeholder amount is rejected by
        // `try_to_transfer_balance_from_message_account` and the refund is
        // returned without `init_transfer_internal` ever being called. (And
        // even if it were called, fix #2 removes the pending entry on
        // storage failure.)
        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": virtual_acct_id }))
            .deposit(placeholder_estimate)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // ft_transfer_call resolves now that the yield has fired.
        let _result = status.await?.into_result()?;

        // ── Verification ───────────────────────────────────────────────────

        // (1) The attacker got a FULL refund. nzec balance is unchanged.
        // Refund still happens on the post-fix code — the difference is just
        // *where* the refund is decided (pre-check rejection in resume vs.
        // storage-check failure inside `init_transfer_internal`).
        let attacker_balance: U128 = nzec_token
            .view("ft_balance_of")
            .args_json(json!({ "account_id": attacker.id() }))
            .await?
            .json()?;
        assert_eq!(
            attacker_balance.0, attacker_initial_balance,
            "attacker should have received a full ft refund"
        );

        // (2) The post-fix invariant: no stale entry in `pending_transfers`.
        // `get_transfer_message` panics with `BridgeError::TransferNotExist`
        // when the id is unknown, so the view call returns an Err.
        let transfer_id = TransferId {
            origin_chain: ChainKind::Near,
            origin_nonce: 1,
        };
        let view_result = env
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({ "transfer_id": transfer_id }))
            .await;
        assert!(
            view_result.is_err(),
            "post-fix: no stale `pending_transfers` entry should remain. \
             Pre-fix bug would leave a TransferMessage with the long Zcash \
             recipient that could be drained via \
             `submit_transfer_to_utxo_chain_connector`. \
             Got: {view_result:?}"
        );

        Ok(())
    }

    /// Counter-example for BTC: the same construction with the LONGEST
    /// possible Bech32m Bitcoin address does NOT trigger the bug, because
    /// real BTC addresses (capped at ~90 chars per BIP-173) are shorter than
    /// the synthetic NEAR-account placeholder used in the pre-check.
    /// The yield resumes successfully, the transfer is fully processed, and
    /// no stale entry is left around.
    #[rstest]
    #[tokio::test]
    async fn test_btc_address_does_not_trigger_stale_transfer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_utxo_token()
            .await?;

        // Same attacker setup as above (named TLA, not implicit).
        let attacker_id: AccountId = "attacker".parse()?;
        let attacker = env.create_account(attacker_id.clone()).await?;

        let required_balance_account: NearToken = env
            .bridge_contract
            .view("required_balance_for_account")
            .await?
            .json()?;
        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": attacker.id() }))
            .deposit(required_balance_account)
            .max_gas()
            .transact()
            .await?
            .into_result()?;
        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": env.bridge_contract.id() }))
            .deposit(NearToken::from_millinear(100))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // Give attacker tokens on the BTC-side token (the one set up by
        // with_utxo_token).
        let attacker_initial_balance: u128 = 100_000;
        env.storage_deposit(attacker.id()).await?;
        env.mint_tokens(attacker.id(), attacker_initial_balance)
            .await?;
        env.bridge_contract
            .call("set_locked_tokens")
            .args_json(json!({
                "args": [{
                    "chain_kind": ChainKind::Btc,
                    "token_id": env.token.contract.id(),
                    "amount": U128(0),
                }]
            }))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        // A maximal-length Bech32m BTC address. BIP-173 caps Bech32 strings
        // at 90 characters. Real wallets use ~62 chars. Both are below the
        // 64-char NEAR placeholder, so the synthetic estimate over-covers
        // the actual cost and the storage check passes.
        let btc_address_90 = format!("bc1p{}", "q".repeat(85)); // length 89
        let recipient = OmniAddress::Btc(btc_address_90.clone());

        let transfer_amount: u128 = 50_000;
        let init_msg = InitTransferMsg {
            recipient: recipient.clone(),
            fee: U128(0),
            native_token_fee: U128(0),
            msg: None,
            external_id: None,
        };

        let storage_account_data = TransferMessageStorageAccount {
            token: OmniAddress::Near(env.token.contract.id().clone()),
            amount: U128(transfer_amount),
            recipient: recipient.clone(),
            fee: Fee::default(),
            sender: OmniAddress::Near(attacker.id().clone()),
            msg: String::new(),
        };
        let virtual_acct_id = storage_account_data.id(None);

        let placeholder_estimate: NearToken = env
            .bridge_contract
            .view("required_balance_for_init_transfer")
            .args_json(json!({}))
            .await?
            .json()?;

        let status = attacker
            .call(env.token.contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": env.bridge_contract.id(),
                "amount": U128(transfer_amount),
                "memo": None::<String>,
                "msg": serde_json::to_string(&BridgeOnTransferMsg::InitTransfer(init_msg))?,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact_async()
            .await?;

        env.worker.fast_forward(5).await?;

        attacker
            .call(env.bridge_contract.id(), "storage_deposit")
            .args_json(json!({ "account_id": virtual_acct_id }))
            .deposit(placeholder_estimate)
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let _result = status.await?.into_result()?;

        // For BTC: the storage check in init_transfer_internal succeeded, so
        // the transfer was fully accepted — the bridge KEEPS the transferred
        // amount. Balance after = initial - transferred (no refund).
        let attacker_balance: U128 = env
            .token
            .contract
            .view("ft_balance_of")
            .args_json(json!({ "account_id": attacker.id() }))
            .await?
            .json()?;
        assert_eq!(
            attacker_balance.0,
            attacker_initial_balance - transfer_amount,
            "BTC path does NOT trigger the bug — bridge legitimately consumed the transferred tokens, \
             so the attacker should be debited (no refund). Compare with the Zcash test where the \
             attacker keeps the full initial balance."
        );

        // And the transfer is queued legitimately, not as a stale leftover —
        // we can sign / process it normally.
        let queued: TransferMessage = env
            .bridge_contract
            .view("get_transfer_message")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: 1,
                }
            }))
            .await?
            .json()?;
        assert_eq!(queued.amount, U128(transfer_amount));

        // Suppress the unused-import warning if account_n is never needed.
        let _ = account_n;

        Ok(())
    }
}
