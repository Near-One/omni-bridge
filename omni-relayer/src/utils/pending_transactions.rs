use omni_types::{ChainKind, near_events::OmniBridgeEvent};
use serde::{Deserialize, Serialize};

pub const PENDING_EVM_TRANSACTIONS_KEY: &str = "pending_evm_transactions";

pub fn get_pending_tx_key(chain_kind: ChainKind) -> String {
    format!("{PENDING_EVM_TRANSACTIONS_KEY}:{chain_kind:?}").to_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub tx_hash: String,
    pub nonce: u64,
    pub sent_timestamp: i64,
    pub last_bump_timestamp: Option<i64>,
    pub chain_kind: ChainKind,
    pub source_event: OmniBridgeEvent,
}

impl PendingTransaction {
    pub fn new(
        tx_hash: String,
        nonce: u64,
        chain_kind: ChainKind,
        source_event: OmniBridgeEvent,
    ) -> Self {
        Self {
            tx_hash,
            nonce,
            sent_timestamp: chrono::Utc::now().timestamp(),
            last_bump_timestamp: None,
            chain_kind,
            source_event,
        }
    }

    pub fn bump(&mut self, tx_hash: String) {
        self.tx_hash = tx_hash;
        self.last_bump_timestamp = Some(chrono::Utc::now().timestamp());
    }
}
