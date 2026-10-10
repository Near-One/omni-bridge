use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, RELAYER_LIST_SEED},
    state::{config::Config, relayer::RelayerList},
};

#[derive(Accounts)]
pub struct InitRelayerList<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        init,
        payer = signer,
        space = RelayerList::space(0),
        seeds = [RELAYER_LIST_SEED],
        bump,
    )]
    pub relayer_list: Box<Account<'info, RelayerList>>,

    #[account(
        mut,
        constraint = signer.key() == config.admin @ crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl InitRelayerList<'_> {
    pub fn process(&mut self, bump: u8) {
        self.relayer_list.bump = bump;
        self.relayer_list.manager = self.signer.key();
    }
}
