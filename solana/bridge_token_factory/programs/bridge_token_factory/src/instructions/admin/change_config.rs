use anchor_lang::prelude::*;

use crate::{constants::CONFIG_SEED, state::config::Config};

#[derive(Accounts)]
pub struct ChangeConfig<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        constraint = signer.key() == config.admin @ crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,
}

impl ChangeConfig<'_> {
    pub fn set_admin(&mut self, admin: Pubkey) -> Result<()> {
        self.config.admin = admin;

        Ok(())
    }

    pub fn set_pausable_admin(&mut self, pausable_admin: Pubkey) -> Result<()> {
        self.config.pausable_admin = pausable_admin;

        Ok(())
    }

    pub fn set_paused(&mut self, paused: u8) -> Result<()> {
        self.config.paused = paused;

        Ok(())
    }

    pub fn set_metadata_admin(&mut self, metadata_admin: Pubkey) -> Result<()> {
        self.config.metadata_admin = metadata_admin;

        Ok(())
    }
}
