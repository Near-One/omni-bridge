use anyhow::Result;

use near_jsonrpc_client::{methods::query::RpcQueryRequest, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::{
    types::{AccountId, BlockReference, Finality},
    views::QueryRequest,
};

pub const NEP141_STORAGE_DEPOSIT: u128 = 1_250_000_000_000_000_000_000;

#[derive(Debug, serde::Deserialize)]
struct StorageBalance {
    total: String,
}

pub async fn is_storage_sufficient(
    jsonrpc_client: &JsonRpcClient,
    token: &AccountId,
    accound_id: &AccountId,
) -> Result<bool> {
    let request = RpcQueryRequest {
        block_reference: BlockReference::Finality(Finality::Final),
        request: QueryRequest::CallFunction {
            account_id: token.clone(),
            method_name: "storage_balance_of".to_string(),
            args: serde_json::json!({ "account_id": accound_id })
                .to_string()
                .into_bytes()
                .into(),
        },
    };

    if let QueryResponseKind::CallResult(result) = jsonrpc_client.call(request).await?.kind {
        if let Ok(storage) = serde_json::from_slice::<StorageBalance>(&result.result) {
            if let Ok(parsed_total) = storage.total.parse::<u128>() {
                return Ok(parsed_total >= NEP141_STORAGE_DEPOSIT);
            }
        }

        Ok(false)
    } else {
        anyhow::bail!("Failed to get storage balance")
    }
}
