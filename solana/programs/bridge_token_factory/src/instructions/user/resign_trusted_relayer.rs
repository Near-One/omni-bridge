use anchor_lang::prelude::*;

use crate::{constants::RELAYER_SEED, error::ErrorCode, state::relayer::RelayerState};

#[derive(Accounts)]
pub struct ResignTrustedRelayer<'info> {
    #[account(
        mut,
        close = signer,
        seeds = [RELAYER_SEED, signer.key().as_ref()],
        bump = relayer_state.bump,
    )]
    pub relayer_state: Box<Account<'info, RelayerState>>,

    #[account(mut)]
    pub signer: Signer<'info>,
}

impl ResignTrustedRelayer<'_> {
    pub fn process(&self) -> Result<()> {
        require!(
            self.relayer_state.is_active(Clock::get()?.unix_timestamp),
            ErrorCode::RelayerNotActive
        );

        Ok(())
    }
}
