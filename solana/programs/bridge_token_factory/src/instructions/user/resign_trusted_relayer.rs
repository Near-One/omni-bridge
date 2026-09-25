use anchor_lang::prelude::*;

use crate::{
    constants::{RELAYER_LIST_SEED, RELAYER_SEED},
    error::ErrorCode,
    state::relayer::{RelayerList, RelayerState},
};

#[derive(Accounts)]
pub struct ResignTrustedRelayer<'info> {
    #[account(
        mut,
        close = signer,
        seeds = [RELAYER_SEED, signer.key().as_ref()],
        bump = relayer_state.bump,
    )]
    pub relayer_state: Box<Account<'info, RelayerState>>,

    #[account(
        mut,
        seeds = [RELAYER_LIST_SEED],
        bump = relayer_list.bump,
        realloc = RelayerList::space(relayer_list.relayers.len().saturating_sub(1)),
        realloc::payer = signer,
        realloc::zero = false,
    )]
    pub relayer_list: Box<Account<'info, RelayerList>>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl ResignTrustedRelayer<'_> {
    pub fn process(&mut self) -> Result<()> {
        require!(
            self.relayer_state.is_active(Clock::get()?.unix_timestamp),
            ErrorCode::RelayerNotActive
        );
        self.relayer_list.remove(&self.signer.key());

        Ok(())
    }
}
