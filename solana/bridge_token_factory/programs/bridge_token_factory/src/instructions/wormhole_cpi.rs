use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};
use wormhole_anchor_sdk::wormhole::{self, program::Wormhole};

use crate::{constants::CONFIG_SEED, state::config::Config};

#[derive(Accounts)]
pub struct WormholeCPI<'info> {
    /// Used as an emitter
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [wormhole::BridgeData::SEED_PREFIX],
        bump = config.bumps.wormhole.bridge,
        seeds::program = wormhole_program.key,
    )]
    /// Wormhole bridge data account (a.k.a. its config).
    /// [`wormhole::post_message`] requires this account be mutable.
    pub bridge: Box<Account<'info, wormhole::BridgeData>>,

    #[account(
        mut,
        seeds = [wormhole::FeeCollector::SEED_PREFIX],
        bump = config.bumps.wormhole.fee_collector,
        seeds::program = wormhole_program.key
    )]
    /// Wormhole fee collector account, which requires lamports before the
    /// program can post a message (if there is a fee).
    /// [`wormhole::post_message`] requires this account be mutable.
    pub fee_collector: Box<Account<'info, wormhole::FeeCollector>>,

    #[account(
        mut,
        seeds = [
            wormhole::SequenceTracker::SEED_PREFIX,
            config.key().as_ref()
        ],
        bump = config.bumps.wormhole.sequence,
        seeds::program = wormhole_program.key
    )]
    /// CHECK: Emitter's sequence account. This is not created until the first
    /// message is posted, so it needs to be an [UncheckedAccount] for the
    /// [`initialize`](crate::initialize) instruction.
    /// [`wormhole::post_message`] requires this account be mutable.
    pub sequence: Account<'info, wormhole::SequenceTracker>,

    /// CHECK: Wormhole Message. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(mut)]
    pub message: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    /// Wormhole program.
    pub wormhole_program: Program<'info, Wormhole>,
    pub system_program: Program<'info, System>,
}

impl<'info> WormholeCPI<'info> {
    pub fn post_message(&self, data: Vec<u8>) -> Result<()> {
        // If Wormhole requires a fee before posting a message, we need to
        // transfer lamports to the fee collector. Otherwise
        // `wormhole::post_message` will fail.
        let fee = self.bridge.fee();
        if fee > 0 {
            transfer(
                CpiContext::new(
                    self.system_program.to_account_info(),
                    Transfer {
                        from: self.payer.to_account_info(),
                        to: self.fee_collector.to_account_info(),
                    },
                ),
                fee,
            )?;
        }

        wormhole::post_message(
            CpiContext::new_with_signer(
                self.wormhole_program.to_account_info(),
                wormhole::PostMessage {
                    config: self.bridge.to_account_info(),
                    message: self.message.to_account_info(),
                    emitter: self.config.to_account_info(),
                    sequence: self.sequence.to_account_info(),
                    payer: self.payer.to_account_info(),
                    fee_collector: self.fee_collector.to_account_info(),
                    clock: self.clock.to_account_info(),
                    rent: self.rent.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                },
                &[
                    &[CONFIG_SEED, &[self.config.bumps.config]], // emitter
                ],
            ),
            0,
            data,
            wormhole::Finality::Finalized,
        )
    }
}
