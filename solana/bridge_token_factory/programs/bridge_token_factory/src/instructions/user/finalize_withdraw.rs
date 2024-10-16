use std::str::FromStr;

use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token_2022::{transfer_checked, TransferChecked}, token_interface::{Mint, TokenAccount, TokenInterface}};
use wormhole_anchor_sdk::wormhole::{post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage, SequenceTracker};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT, USED_NONCES_SEED, VAULT_SEED
    }, error::ErrorCode, state::{config::Config, used_nonces::UsedNonces}, FinalizeDepositData
};

use super::FinalizeDepositResponse;

#[derive(Accounts)]
#[instruction(data: FinalizeDepositData)]
pub struct FinalizeWithdraw<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Account<'info, Config>,
    #[account(
        init_if_needed,
        space = USED_NONCES_ACCOUNT_SIZE,
        payer = payer,
        seeds = [
            USED_NONCES_SEED,
            &(data.payload.nonce / USED_NONCES_PER_ACCOUNT as u128).to_le_bytes(),
        ],
        bump,
    )]
    pub used_nonces: AccountLoader<'info, UsedNonces>,
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        constraint = recipient.key == &data.payload.recipient,
    )]
    /// CHECK: this can be any type of account
    pub recipient: UncheckedAccount<'info>,

    #[account(
        address = Pubkey::from_str(&data.payload.token).or(err!(ErrorCode::SolanaTokenParsingFailed))?,
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    // if this account exists the mint registration is already sent
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

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
        token::token_program = token_program,
    )]
    pub token_account: InterfaceAccount<'info, TokenAccount>,

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

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> FinalizeWithdraw<'info> {
    pub fn process(&mut self, data: FinalizeDepositData, wormhole_message_bump: u8) -> Result<()> {
        UsedNonces::use_nonce(
            data.payload.nonce,
            &self.used_nonces,
            &mut self.config,
            self.payer.to_account_info(),
            &Rent::get()?,
            self.system_program.to_account_info(),
        )?;

        let bump = &[self.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.vault.to_account_info(),
                    to: self.token_account.to_account_info(),
                    authority: self.authority.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
                signer_seeds,
            ),
            data.payload.amount.try_into().unwrap(),
            self.mint.decimals,
        )?;

        let payload = FinalizeDepositResponse {
            nonce: data.payload.nonce,
        }.try_to_vec()?;

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

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeWithdrawResponse {
    pub nonce: u128,
}