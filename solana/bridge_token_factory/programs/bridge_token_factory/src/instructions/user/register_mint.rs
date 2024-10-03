use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::MetadataAccount,
    token::{Mint, Token, TokenAccount},
};
use wormhole_anchor_sdk::wormhole::{post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage, SequenceTracker};

use super::MetadataPayload;
use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED, VAULT_SEED},
    state::config::Config,
};
use anchor_spl::metadata::ID as MetaplexID;

#[derive(Accounts)]
pub struct RegisterMint<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Account<'info, Config>,
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        seeds = [
            b"metadata",
            MetaplexID.as_ref(),
            &mint.key().to_bytes(),
        ],
        bump,
        seeds::program = MetaplexID,
    )]
    pub metadata: Account<'info, MetadataAccount>,

    #[account(
        init,
        payer = payer,
        token::mint = mint,
        token::authority = authority,
        seeds = [
            VAULT_SEED,
            mint.key().as_ref(),
        ],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// Wormhole bridge data. [`wormhole::post_message`] requires this account
    /// be mutable.
    #[account(
        address = config.wormhole.bridge,
    )]
    pub wormhole_bridge: Account<'info, BridgeData>,

    /// Wormhole fee collector. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.fee_collector
    )]
    pub wormhole_fee_collector: Account<'info, FeeCollector>,

    /// Emitter's sequence account. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.sequence
    )]
    pub wormhole_sequence: Account<'info, SequenceTracker>,

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
    pub wormhole_message: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> RegisterMint<'info> {
    pub fn process(&mut self, wormhole_message_bump: u8) -> Result<()> {
        // TODO: correct message payload
        let payload = MetadataPayload {
            token: self.mint.key().to_string(),
            name: self.metadata.name.clone(),
            symbol: self.metadata.symbol.clone(),
            decimals: self.mint.decimals,
        }
        .try_to_vec()?;

        post_message(
            CpiContext::new_with_signer(
                self.wormhole_program.to_account_info(),
                PostMessage {
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
