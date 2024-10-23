use crate::instructions::wormhole_cpi::*;
use anchor_lang::{
    prelude::*,
    solana_program::{keccak, secp256k1_recover::secp256k1_recover},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};
use near_sdk::json_types::U128;
use std::{
    io::{BufWriter, Write},
    vec,
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT,
        USED_NONCES_SEED,
    },
    error::ErrorCode,
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
        payer = wormhole.payer,
        seeds = [
            USED_NONCES_SEED,
            &(data.payload.nonce / USED_NONCES_PER_ACCOUNT as u128).to_le_bytes(),
        ],
        bump,
    )]
    pub used_nonces: AccountLoader<'info, UsedNonces>,

    #[account(
        address = data.payload.recipient,
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
        payer = wormhole.payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub token_account: Box<Account<'info, TokenAccount>>,

    pub wormhole: WormholeCPI<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

impl<'info> FinalizeDeposit<'info> {
    pub fn mint(&mut self, data: FinalizeDepositData, wormhole_message_bump: u8) -> Result<()> {
        UsedNonces::use_nonce(
            data.payload.nonce,
            &self.used_nonces,
            &mut self.config,
            self.wormhole.payer.to_account_info(),
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
        }
        .try_to_vec()?;

        self.wormhole.post_message(payload, wormhole_message_bump)?;

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
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeDepositData {
    pub payload: DepositPayload,
    signature: [u8; 65],
}

impl FinalizeDepositData {
    pub fn verify_signature(&self, derived_near_bridge_address: &[u8; 64]) -> Result<()> {
        let borsh_encoded = self.payload.serialize_for_signature()?;
        let hash = keccak::hash(&borsh_encoded);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| error!(ErrorCode::SignatureVerificationFailed))?;

        require!(
            signer.0 == *derived_near_bridge_address,
            ErrorCode::SignatureVerificationFailed
        );

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeDepositResponse {
    pub nonce: u128,
}
