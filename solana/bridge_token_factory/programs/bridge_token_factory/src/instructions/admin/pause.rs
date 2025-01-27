use anchor_lang::prelude::*;

use crate::{
    constants::{ALL_PAUSED, CONFIG_SEED},
    state::config::Config,
};

#[derive(Accounts)]
pub struct Pause<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        constraint = signer.key() == config.pausable_admin || signer.key() == config.admin @ 
            crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,
}

impl<'info> Pause<'info> {
    pub fn process(&mut self) -> Result<()> {
        self.config.paused = ALL_PAUSED;

        Ok(())
    }
}
