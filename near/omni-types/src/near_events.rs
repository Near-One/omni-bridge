use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;

use crate::mpc_types::SignatureResponse;
use crate::{
    ClaimNativeFeePayload, MetadataPayload, OmniAddress, TransferMessage, TransferMessagePayload,
};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum Nep141LockerEvent {
    InitTransferEvent {
        transfer_message: TransferMessage,
    },
    SignTransferEvent {
        signature: SignatureResponse,
        message_payload: TransferMessagePayload,
    },
    FinTransferEvent {
        nonce: Option<U128>,
        transfer_message: TransferMessage,
    },
    UpdateFeeEvent {
        transfer_message: TransferMessage,
    },
    LogMetadataEvent {
        signature: SignatureResponse,
        metadata_payload: MetadataPayload,
    },
    SignClaimNativeFeeEvent {
        signature: SignatureResponse,
        claim_payload: ClaimNativeFeePayload,
    },
    ClaimFeeEvent {
        transfer_message: TransferMessage,
    },
}

impl Nep141LockerEvent {
    pub fn to_log_string(&self) -> String {
        json!(self).to_string()
    }
}
