use near_primitives::types::AccountId;
use near_sdk::json_types::U128;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DepositMsg {
    pub recipient_id: AccountId,
    pub post_actions: Option<Vec<PostAction>>,
    pub extra_msg: Option<String>,
    pub safe_deposit: Option<SafeDepositMsg>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PostAction {
    pub receiver_id: AccountId,
    pub amount: U128,
    pub memo: Option<String>,
    pub msg: String,
    pub gas: Option<u64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SafeDepositMsg {
    pub msg: String,
}
