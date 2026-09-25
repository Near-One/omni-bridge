use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{
    constants::{CONFIG_SEED, RELAYER_SEED},
    error::ErrorCode,
    state::{config::Config, relayer::RelayerState},
};

#[derive(Accounts)]
pub struct ApplyForTrustedRelayer<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        init,
        payer = signer,
        space = 8 + RelayerState::INIT_SPACE,
        seeds = [RELAYER_SEED, signer.key().as_ref()],
        bump,
    )]
    pub relayer_state: Box<Account<'info, RelayerState>>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl ApplyForTrustedRelayer<'_> {
    pub fn process(&mut self, bump: u8) -> Result<()> {
        let stake = self.config.relayer_stake_required;
        require!(stake > 0, ErrorCode::RelayerStakingDisabled);

        let activate_at = Clock::get()?
            .unix_timestamp
            .checked_add(self.config.relayer_waiting_period)
            .ok_or_else(|| error!(ErrorCode::InvalidArgs))?;

        transfer(
            CpiContext::new(
                self.system_program.to_account_info(),
                Transfer {
                    from: self.signer.to_account_info(),
                    to: self.relayer_state.to_account_info(),
                },
            ),
            stake,
        )?;

        self.relayer_state.set_inner(RelayerState {
            stake,
            activate_at,
            bump,
        });

        Ok(())
    }
}
