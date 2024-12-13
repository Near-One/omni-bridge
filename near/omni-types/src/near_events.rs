use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;

use crate::mpc_types::SignatureResponse;
use crate::{FastTransfer, MetadataPayload, TransferId, TransferMessage, TransferMessagePayload};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum OmniBridgeEvent {
    InitTransferEvent {
        transfer_message: TransferMessage,
    },
    SignTransferEvent {
        signature: SignatureResponse,
        message_payload: TransferMessagePayload,
    },
    FinTransferEvent {
        transfer_message: TransferMessage,
    },
    UpdateFeeEvent {
        transfer_message: TransferMessage,
    },
    LogMetadataEvent {
        signature: SignatureResponse,
        metadata_payload: MetadataPayload,
    },
    ClaimFeeEvent {
        transfer_message: TransferMessage,
    },
    FastTransferEvent {
        fast_transfer: FastTransfer,
        new_transfer_id: Option<TransferId>,
    },
}

impl OmniBridgeEvent {
    pub fn to_log_string(&self) -> String {
        json!(self).to_string()
    }
}
