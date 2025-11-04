use near_sdk::serde_json::json;
use near_sdk::{near, AccountId};

use crate::mpc_types::SignatureResponse;
use crate::{
    BasicMetadata, FastTransfer, MetadataPayload, OmniAddress, TransferId, TransferMessage,
    TransferMessagePayload, UtxoFinTransferMsg,
};

#[near(serializers=[json])]
#[derive(Clone, Debug)]
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
    FailedFinTransferEvent {
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
    DeployTokenEvent {
        token_id: AccountId,
        token_address: OmniAddress,
        metadata: BasicMetadata,
    },
    BindTokenEvent {
        token_id: AccountId,
        token_address: OmniAddress,
        decimals: u8,
        origin_decimals: u8,
    },
    FastTransferEvent {
        fast_transfer: FastTransfer,
        new_transfer_id: Option<TransferId>,
    },
    UtxoTransferEvent {
        utxo_transfer_message: UtxoFinTransferMsg,
        new_transfer_id: Option<TransferId>,
    },
}

impl OmniBridgeEvent {
    pub fn to_log_string(&self) -> String {
        json!(self).to_string()
    }
}
