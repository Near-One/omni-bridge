use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, RELAYER_SEED},
    state::{config::Config, relayer::RelayerState},
};

#[derive(Accounts)]
#[instruction(relayer: Pubkey)]
pub struct RejectRelayerApplication<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        close = signer,
        seeds = [RELAYER_SEED, relayer.as_ref()],
        bump = relayer_state.bump,
    )]
    pub relayer_state: Box<Account<'info, RelayerState>>,

    #[account(
        mut,
        constraint = signer.key() == config.admin @ crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,
}
