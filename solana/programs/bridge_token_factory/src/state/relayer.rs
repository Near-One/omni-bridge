use anchor_lang::prelude::*;

use crate::constants::MAX_RELAYERS_PER_PAGE;

#[account]
#[derive(InitSpace)]
pub struct RelayerState {
    pub stake: u64,
    pub activate_at: i64,
    pub bump: u8,
}

impl RelayerState {
    pub const fn is_active(&self, now: i64) -> bool {
        now >= self.activate_at
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct RelayerEntry {
    pub relayer: Pubkey,
    pub stake: u64,
    pub activate_at: i64,
}

// Holds only rent: shrinking it sends every lamport above rent to the signer
#[account]
pub struct RelayerList {
    pub bump: u8,
    pub manager: Pubkey,
    pub relayers: Vec<RelayerEntry>,
}

impl RelayerList {
    pub const fn space(len: usize) -> usize {
        8 + 1 + 32 + 4 + len * RelayerEntry::INIT_SPACE
    }

    pub fn remove(&mut self, relayer: &Pubkey) {
        self.relayers.retain(|entry| entry.relayer != *relayer);
    }

    pub fn page(&self, active: bool, from_index: u32, limit: u32) -> Result<Vec<RelayerEntry>> {
        let now = Clock::get()?.unix_timestamp;
        Ok(self
            .relayers
            .iter()
            .filter(|entry| (now >= entry.activate_at) == active)
            .skip(usize::try_from(from_index).unwrap_or(usize::MAX))
            .take(usize::try_from(limit.min(MAX_RELAYERS_PER_PAGE)).unwrap_or(0))
            .cloned()
            .collect())
    }
}
