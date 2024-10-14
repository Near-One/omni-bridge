use anchor_lang::prelude::*;
use wormhole_anchor_sdk::wormhole::{BridgeData, FeeCollector, SequenceTracker};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, DEFAULT_ADMIN, USED_NONCES_PER_ACCOUNT},
    state::{
        config::{Config, ConfigBumps, WormholeConfig},
        used_nonces::UsedNonces,
    },
    ID,
};
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(address = DEFAULT_ADMIN)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,

    pub wormhole_bridge: Account<'info, BridgeData>,
    pub wormhole_fee_collector: Account<'info, FeeCollector>,
    pub wormhole_sequence: Account<'info, SequenceTracker>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn process(
        &mut self,
        derived_near_bridge_address: [u8; 64],
        config_bump: u8,
    ) -> Result<()> {
        let (_, authority_bump) = Pubkey::find_program_address(&[AUTHORITY_SEED], &ID);
        self.config.set_inner(Config {
            admin: DEFAULT_ADMIN,
            max_used_nonce: 0,
            derived_near_bridge_address,
            wormhole: WormholeConfig {
                bridge: self.wormhole_bridge.key(),
                fee_collector: self.wormhole_fee_collector.key(),
                sequence: self.wormhole_sequence.key(),
            },
            bumps: ConfigBumps {
                config: config_bump,
                authority: authority_bump,
            },
        });

        // prepare rent for the next used_nonces account creation
        transfer(
            CpiContext::new(
                self.system_program.to_account_info(),
                Transfer {
                    from: self.payer.to_account_info(),
                    to: self.config.to_account_info(),
                },
            ),
            UsedNonces::rent_level(USED_NONCES_PER_ACCOUNT as u128 - 1, &Rent::get()?)?,
        )?;
        Ok(())
    }
}
