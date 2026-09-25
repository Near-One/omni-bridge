use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, RELAYER_LIST_SEED},
    state::{config::Config, relayer::RelayerList},
};

#[derive(Accounts)]
pub struct SetRelayerManager<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [RELAYER_LIST_SEED],
        bump = relayer_list.bump,
    )]
    pub relayer_list: Box<Account<'info, RelayerList>>,

    #[account(
        constraint = signer.key() == config.admin @ crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,
}

impl SetRelayerManager<'_> {
    pub fn process(&mut self, manager: Pubkey) {
        self.relayer_list.manager = manager;
    }
}
