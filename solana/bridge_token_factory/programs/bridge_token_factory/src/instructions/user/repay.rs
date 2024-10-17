use anchor_lang::prelude::*;
use anchor_spl::token::{burn, Burn, Mint, Token, TokenAccount};
use wormhole_anchor_sdk::wormhole::{
    post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage,
    SequenceTracker,
};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED},
    state::config::Config,
};

#[derive(Accounts)]
#[instruction(payload: RepayPayload)]
pub struct Repay<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [payload.token.as_bytes().as_ref()],
        bump,
        mint::authority = authority,
    )]
    pub mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        token::mint = mint,
    )]
    pub from: Box<Account<'info, TokenAccount>>,
    pub user: Signer<'info>,

    /// Wormhole bridge data. [`wormhole::post_message`] requires this account
    /// be mutable.
    #[account(
        mut,
        address = config.wormhole.bridge,
    )]
    pub wormhole_bridge: Box<Account<'info, BridgeData>>,

    /// Wormhole fee collector. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.fee_collector
    )]
    pub wormhole_fee_collector: Box<Account<'info, FeeCollector>>,

    /// Emitter's sequence account. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.sequence
    )]
    pub wormhole_sequence: Box<Account<'info, SequenceTracker>>,

    /// CHECK: Wormhole Message. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        seeds = [
            MESSAGE_SEED,
            &wormhole_sequence.next_value().to_le_bytes()[..]
        ],
        bump,
    )]
    pub wormhole_message: SystemAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub wormhole_program: Program<'info, Wormhole>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RepayPayload {
    pub token: String,
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

impl<'info> Repay<'info> {
    pub fn process(&self, payload: RepayPayload, wormhole_message_bump: u8) -> Result<()> {
        burn(
            CpiContext::new(
                self.token_program.to_account_info(),
                Burn {
                    mint: self.mint.to_account_info(),
                    from: self.from.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            payload.amount.try_into().unwrap(),
        )?;

        let payload = payload.try_to_vec()?; // TODO: correct message payload

        post_message(
            CpiContext::new_with_signer(
                self.wormhole_program.to_account_info(),
                PostMessage {
                    config: self.wormhole_bridge.to_account_info(),
                    message: self.wormhole_message.to_account_info(),
                    emitter: self.config.to_account_info(),
                    sequence: self.wormhole_sequence.to_account_info(),
                    payer: self.user.to_account_info(),
                    fee_collector: self.wormhole_fee_collector.to_account_info(),
                    clock: self.clock.to_account_info(),
                    rent: self.rent.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                },
                &[
                    &[
                        MESSAGE_SEED,
                        &self.wormhole_sequence.next_value().to_le_bytes()[..],
                        &[wormhole_message_bump],
                    ],
                    &[CONFIG_SEED, &[self.config.bumps.config]], // emitter
                ],
            ),
            0,
            payload,
            Finality::Finalized,
        )?;

        Ok(())
    }
}
