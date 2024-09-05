use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;

use crate::mpc_types::SignatureResponse;
use crate::{TransferMessage, TransferMessagePayload};

#[derive(Deserialize, Serialize, Clone)]
pub enum Nep141LockerEvent {
    InitTransferEvent {
        transfer_message: TransferMessage,
    },
    SignTransferEvent {
        signature: SignatureResponse,
        message_payload: TransferMessagePayload,
    },
}

impl Nep141LockerEvent {
    pub fn to_log_string(&self) -> String {
        json!(self).to_string()
    }
}
