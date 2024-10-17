use anchor_lang::{
    prelude::*,
    solana_program::{keccak, secp256k1_recover::secp256k1_recover},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};
use near_sdk::json_types::U128;
use wormhole_anchor_sdk::wormhole::{post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage, SequenceTracker};
use std::{
    io::{BufWriter, Write},
    vec,
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT, USED_NONCES_SEED
    },
    state::{config::Config, used_nonces::UsedNonces},
};

#[derive(Accounts)]
#[instruction(data: FinalizeDepositData)]
pub struct FinalizeDeposit<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,
    #[account(
        init_if_needed,
        space = USED_NONCES_ACCOUNT_SIZE as usize,
        payer = payer,
        seeds = [
            USED_NONCES_SEED,
            &(data.payload.nonce / USED_NONCES_PER_ACCOUNT as u128).to_le_bytes(),
        ],
        bump,
    )]
    pub used_nonces: AccountLoader<'info, UsedNonces>,

    #[account(
        constraint = recipient.key == &data.payload.recipient,
    )]
    /// CHECK: this can be any type of account
    pub recipient: UncheckedAccount<'info>,
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [data.payload.token.as_bytes().as_ref()],
        bump,
        mint::authority = authority,
    )]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub payer: Signer<'info>,

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

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> FinalizeDeposit<'info> {
    pub fn mint(&mut self, data: FinalizeDepositData, wormhole_message_bump: u8) -> Result<()> {
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

        let cpi_accounts = MintTo {
            mint: self.mint.to_account_info(),
            to: self.token_account.to_account_info(),
            authority: self.authority.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        mint_to(cpi_ctx, data.payload.amount.try_into().unwrap())?;

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
pub struct DepositPayload {
    pub nonce: u128,
    pub token: String,
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

impl DepositPayload {
    fn serialize_for_signature(&self) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(vec![]);
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        self.token.serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.amount), &mut writer)?;
        writer.write(&[2])?;
        self.recipient.to_string().serialize(&mut writer)?;
        self.fee_recipient.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| crate::ErrorCode::InvalidArgs.into())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeDepositData {
    pub payload: DepositPayload,
    signature: [u8; 65],
}

impl FinalizeDepositData {
    pub fn verify_signature(&self) -> Result<()> {
        let borsh_encoded = self.payload.serialize_for_signature()?;
        let hash = keccak::hash(&borsh_encoded);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| crate::ErrorCode::SignatureVerificationFailed)?;

        require!(
            signer.0 == crate::DERIVED_NEAR_BRIDGE_ADDRESS,
            crate::ErrorCode::SignatureVerificationFailed
        );

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeDepositResponse {
    pub nonce: u128,
}