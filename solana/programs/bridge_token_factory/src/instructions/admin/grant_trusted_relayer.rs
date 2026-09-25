use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, RELAYER_LIST_SEED, RELAYER_SEED},
    state::{
        config::Config,
        relayer::{RelayerEntry, RelayerList, RelayerState},
    },
};

#[derive(Accounts)]
#[instruction(relayer: Pubkey)]
pub struct GrantTrustedRelayer<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        init,
        payer = signer,
        space = 8 + RelayerState::INIT_SPACE,
        seeds = [RELAYER_SEED, relayer.as_ref()],
        bump,
    )]
    pub relayer_state: Box<Account<'info, RelayerState>>,

    #[account(
        mut,
        seeds = [RELAYER_LIST_SEED],
        bump = relayer_list.bump,
        realloc = RelayerList::space(relayer_list.relayers.len() + 1),
        realloc::payer = signer,
        realloc::zero = false,
    )]
    pub relayer_list: Box<Account<'info, RelayerList>>,

    #[account(
        mut,
        constraint = signer.key() == config.admin @ crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl GrantTrustedRelayer<'_> {
    pub fn process(&mut self, relayer: Pubkey, bump: u8) -> Result<()> {
        let activate_at = Clock::get()?.unix_timestamp;

        self.relayer_state.set_inner(RelayerState {
            stake: 0,
            activate_at,
            bump,
        });
        self.relayer_list.relayers.push(RelayerEntry {
            relayer,
            stake: 0,
            activate_at,
        });

        Ok(())
    }
}
