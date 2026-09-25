use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, RELAYER_LIST_SEED, RELAYER_SEED},
    state::{
        config::Config,
        relayer::{RelayerList, RelayerState},
    },
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
        seeds = [RELAYER_LIST_SEED],
        bump = relayer_list.bump,
        realloc = RelayerList::space(relayer_list.relayers.len().saturating_sub(1)),
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

impl RejectRelayerApplication<'_> {
    pub fn process(&mut self, relayer: &Pubkey) {
        self.relayer_list.remove(relayer);
    }
}
