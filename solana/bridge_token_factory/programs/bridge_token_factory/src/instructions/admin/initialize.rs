use anchor_lang::prelude::*;

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, DEFAULT_ADMIN, MESSAGE_SEED, USED_NONCES_PER_ACCOUNT,
    },
    state::{
        config::{Config, ConfigBumps, WormholeBumps, WormholeConfig},
        used_nonces::UsedNonces,
    },
    ID,
};
use anchor_lang::system_program::{transfer, Transfer};
use wormhole_anchor_sdk::wormhole::{self, program::Wormhole};

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

    #[account(
        mut,
        seeds = [wormhole::BridgeData::SEED_PREFIX],
        bump,
        seeds::program = wormhole_program.key,
    )]
    /// Wormhole bridge data account (a.k.a. its config).
    /// [`wormhole::post_message`] requires this account be mutable.
    pub wormhole_bridge: Box<Account<'info, wormhole::BridgeData>>,

    #[account(
        mut,
        seeds = [wormhole::FeeCollector::SEED_PREFIX],
        bump,
        seeds::program = wormhole_program.key
    )]
    /// Wormhole fee collector account, which requires lamports before the
    /// program can post a message (if there is a fee).
    /// [`wormhole::post_message`] requires this account be mutable.
    pub wormhole_fee_collector: Box<Account<'info, wormhole::FeeCollector>>,

    #[account(
        mut,
        seeds = [
            wormhole::SequenceTracker::SEED_PREFIX,
            config.key().as_ref()
        ],
        bump,
        seeds::program = wormhole_program.key
    )]
    /// CHECK: Emitter's sequence account. This is not created until the first
    /// message is posted, so it needs to be an [UncheckedAccount] for the
    /// [`initialize`](crate::initialize) instruction.
    /// [`wormhole::post_message`] requires this account be mutable.
    pub wormhole_sequence: UncheckedAccount<'info>,

    /// CHECK: Wormhole Message. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        seeds = [
            MESSAGE_SEED,
            &wormhole::INITIAL_SEQUENCE.to_le_bytes()[..]
        ],
        bump,
    )]
    pub wormhole_message: SystemAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> Initialize<'info> {
    pub fn process(
        &mut self,
        derived_near_bridge_address: [u8; 64],
        config_bump: u8,
        wormhole_bridge_bump: u8,
        wormhole_fee_collector_bump: u8,
        wormhole_sequence_bump: u8,
        wormhole_message_bump: u8,
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
                wormhole: WormholeBumps {
                    bridge: wormhole_bridge_bump,
                    fee_collector: wormhole_fee_collector_bump,
                    sequence: wormhole_sequence_bump,
                },
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

        // If Wormhole requires a fee before posting a message, we need to
        // transfer lamports to the fee collector. Otherwise
        // `wormhole::post_message` will fail.
        let fee = self.wormhole_bridge.fee();
        if fee > 0 {
            transfer(
                CpiContext::new(
                    self.system_program.to_account_info(),
                    Transfer {
                        from: self.payer.to_account_info(),
                        to: self.wormhole_fee_collector.to_account_info(),
                    },
                ),
                fee,
            )?;
        }

        let payload = vec![]; // TODO: correct message payload

        wormhole::post_message(
            CpiContext::new_with_signer(
                self.wormhole_program.to_account_info(),
                wormhole::PostMessage {
                    config: self.wormhole_bridge.to_account_info(),
                    message: self.wormhole_message.to_account_info(),
                    emitter: self.config.to_account_info(),
                    sequence: self.wormhole_sequence.to_account_info(),
                    payer: self.payer.to_account_info(),
                    fee_collector: self.wormhole_fee_collector.to_account_info(),
                    clock: self.clock.to_account_info(),
                    rent: self.rent.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                },
                &[
                    &[
                        MESSAGE_SEED,
                        &wormhole::INITIAL_SEQUENCE.to_le_bytes()[..],
                        &[wormhole_message_bump],
                    ],
                    &[CONFIG_SEED, &[self.config.bumps.config]], // emitter
                ],
            ),
            0,
            payload,
            wormhole::Finality::Finalized,
        )?;

        Ok(())
    }
}
