use anchor_lang::prelude::*;

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
