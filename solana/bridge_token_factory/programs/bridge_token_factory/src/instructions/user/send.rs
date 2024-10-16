use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use wormhole_anchor_sdk::wormhole::{
    post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage,
    SequenceTracker,
};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED, VAULT_SEED},
    state::config::Config,
};

#[derive(Accounts)]
pub struct Send<'info> {
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
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::token_program = token_program,
    )]
    pub from: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority,
        seeds = [
            VAULT_SEED,
            mint.key().as_ref(),
        ],
        bump,
        token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub user: Signer<'info>,

    /// Wormhole bridge data. [`wormhole::post_message`] requires this account
    /// be mutable.
    #[account(
        mut,
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

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub wormhole_program: Program<'info, Wormhole>,
}

#[derive(AnchorDeserialize, AnchorSerialize, Clone, Default)]
pub struct SendData {
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SendPayload {
    pub token: String,
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

impl<'info> Send<'info> {
    pub fn process(&self, data: SendData, wormhole_message_bump: u8) -> Result<()> {
        transfer_checked(
            CpiContext::new(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.from.to_account_info(),
                    to: self.vault.to_account_info(),
                    authority: self.user.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            data.amount.try_into().unwrap(),
            self.mint.decimals,
        )?;

        let payload = SendPayload {
            token: self.mint.key().to_string(),
            amount: data.amount,
            recipient: data.recipient,
            fee_recipient: data.fee_recipient,
        }
        .try_to_vec()?; // TODO: correct message payload

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
