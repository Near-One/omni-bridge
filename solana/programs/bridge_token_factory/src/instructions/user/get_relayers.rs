use anchor_lang::prelude::*;

use crate::{constants::RELAYER_LIST_SEED, state::relayer::RelayerList};

#[derive(Accounts)]
pub struct GetRelayers<'info> {
    #[account(
        seeds = [RELAYER_LIST_SEED],
        bump = relayer_list.bump,
    )]
    pub relayer_list: Box<Account<'info, RelayerList>>,
}
