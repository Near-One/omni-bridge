use anchor_lang::prelude::*;

use crate::constants::CONFIG_SEED;

#[derive(Accounts)]
pub struct EmitMessage<'info> {
    #[account(seeds = [CONFIG_SEED], bump)]
    pub config: Signer<'info>,
}
