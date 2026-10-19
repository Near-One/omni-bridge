use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId};

use crate::mpc_types::SignatureResponse;
use crate::{
    BasicMetadata, FastTransfer, MetadataPayload, OmniAddress, TransferId, TransferMessage,
    TransferMessagePayload, UtxoFinTransferMsg,
};

/// The NEAR runtime's `max_total_log_length`: the combined byte length of every log a single
/// receipt emits. Exceeding it raises `HostError::TotalLogLengthExceeded`, which aborts the
/// whole receipt — so an informational log can strand a transfer.
///
/// Source: `near-parameters`, `res/runtime_configs/parameters.yaml` (`max_total_log_length`),
/// identical on mainnet and testnet.
///
/// Every field reachable from [`OmniBridgeEvent`] is length-bounded at the type level so a
/// single event cannot approach this; [`OmniBridgeEvent::emit`] enforces the per-receipt
/// total at runtime. See `near/CLAUDE.md` ("Bounded sizes") before adding a logged field.
pub const NEAR_MAX_TOTAL_LOG_LENGTH: usize = 16_384;

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
        token_id: AccountId,
        amount: U128,
        utxo_transfer_message: UtxoFinTransferMsg,
        new_transfer_id: Option<TransferId>,
    },
    MigrateTokenEvent {
        old_token_id: AccountId,
        new_token_id: AccountId,
    },
}

impl OmniBridgeEvent {
    pub fn to_log_string(&self) -> String {
        json!(self).to_string()
    }

    /// Short identifier used by the fallback log when a full event will not fit.
    const fn kind(&self) -> &'static str {
        match self {
            Self::InitTransferEvent { .. } => "InitTransferEvent",
            Self::SignTransferEvent { .. } => "SignTransferEvent",
            Self::FinTransferEvent { .. } => "FinTransferEvent",
            Self::FailedFinTransferEvent { .. } => "FailedFinTransferEvent",
            Self::UpdateFeeEvent { .. } => "UpdateFeeEvent",
            Self::LogMetadataEvent { .. } => "LogMetadataEvent",
            Self::ClaimFeeEvent { .. } => "ClaimFeeEvent",
            Self::DeployTokenEvent { .. } => "DeployTokenEvent",
            Self::BindTokenEvent { .. } => "BindTokenEvent",
            Self::FastTransferEvent { .. } => "FastTransferEvent",
            Self::UtxoTransferEvent { .. } => "UtxoTransferEvent",
            Self::MigrateTokenEvent { .. } => "MigrateTokenEvent",
        }
    }

    /// Emit this event as a contract log, without ever risking the receipt.
    ///
    /// Bounded field types keep any single event well under
    /// [`NEAR_MAX_TOTAL_LOG_LENGTH`], but the runtime limit applies per *receipt*: a
    /// function emitting two near-maximal events, or one event plus other `env::log_str`
    /// calls, could still overflow. Rather than aborting the receipt — which would strand a
    /// transfer over a log line — an event that no longer fits degrades to a short marker
    /// naming what was dropped.
    pub fn emit(&self) {
        let log = self.to_log_string();
        if log_budget::try_reserve(log.len()) {
            env::log_str(&log);
        } else {
            // Bounded by construction: a fixed template plus a &'static str.
            env::log_str(&format!(
                r#"{{"OmittedEvent":{{"kind":"{}","reason":"receipt log budget exhausted"}}}}"#,
                self.kind()
            ));
        }
    }
}

/// Tracks how much of the receipt's log allowance has been spent.
///
/// Contract Wasm is single-threaded and each receipt gets a fresh instance, so a plain
/// counter is sound and correctly resets between receipts.
mod log_budget {
    use super::NEAR_MAX_TOTAL_LOG_LENGTH;
    use std::cell::Cell;

    /// Headroom left for logs this module does not mediate (`env::log_str` called directly,
    /// plus the fallback marker itself).
    const RESERVED: usize = 512;

    thread_local! {
        static SPENT: Cell<usize> = const { Cell::new(0) };
    }

    /// Charges `len` against the receipt's budget, returning whether it fit.
    pub fn try_reserve(len: usize) -> bool {
        SPENT.with(|spent| {
            let next = spent.get().saturating_add(len);
            if next > NEAR_MAX_TOTAL_LOG_LENGTH.saturating_sub(RESERVED) {
                false
            } else {
                spent.set(next);
                true
            }
        })
    }
}
