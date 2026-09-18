#[cfg(test)]
mod tests {
    use near_sdk::serde_json::{json, Value};
    use near_workspaces::types::NearToken;
    use omni_types::{ChainKind, TransferId};
    use rstest::rstest;

    use crate::{
        environment::TestEnvBuilder,
        helpers::tests::{build_artifacts, BuildArtifacts},
    };

    /// The DAO can put back a transfer message that was deleted from the storage, e.g. a transfer
    /// to a UTXO chain that was signed via `sign_transfer` before that was forbidden.
    ///
    /// The transfer message is passed as raw JSON, the way the DAO composes it, so the test pins
    /// the JSON shape the contract accepts.
    #[rstest]
    #[tokio::test]
    async fn restore_transfer_message_restores_deleted_transfer(
        build_artifacts: &BuildArtifacts,
    ) -> anyhow::Result<()> {
        let env = TestEnvBuilder::new(build_artifacts.clone())
            .await?
            .with_utxo_token()
            .await?;

        let transfer_message = json!({
            "origin_nonce": 1,
            "token": "near:btc-token",
            "amount": "100000000",
            "recipient": "btc:bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
            "fee": {
                "fee": "1000",
                "native_fee": "0",
            },
            "sender": "near:account_1",
            "msg": "",
            "destination_nonce": 1,
            "origin_transfer_id": null,
        });
        let args = json!({
            "transfer_message": transfer_message,
        });

        env.bridge_contract
            .call("restore_transfer_message")
            .args_json(&args)
            .deposit(NearToken::from_near(1))
            .max_gas()
            .transact()
            .await?
            .into_result()?;

        let stored: Value = env
            .bridge_contract
            .view("get_transfer_message_storage")
            .args_json(json!({
                "transfer_id": TransferId {
                    origin_chain: ChainKind::Near,
                    origin_nonce: 1,
                }
            }))
            .await?
            .json()?;
        assert_eq!(
            stored,
            json!({
                "message": transfer_message,
                "owner": "account_1",
            }),
            "The restored message should be stored as it was passed"
        );

        // A message that is already in the storage can't be overwritten.
        let duplicate_result = env
            .bridge_contract
            .call("restore_transfer_message")
            .args_json(&args)
            .deposit(NearToken::from_near(1))
            .max_gas()
            .transact()
            .await?;
        assert!(
            format!("{:?}", duplicate_result.failures()).contains("ERR_KEY_EXISTS"),
            "Restoring an existing transfer message should fail: {:?}",
            duplicate_result.failures()
        );

        Ok(())
    }
}
