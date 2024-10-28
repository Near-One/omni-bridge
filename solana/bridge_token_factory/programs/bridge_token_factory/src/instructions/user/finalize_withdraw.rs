use std::str::FromStr;

use anchor_lang::{
    prelude::*,
    solana_program::{keccak, secp256k1_recover::secp256k1_recover},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use near_sdk::json_types::U128;
use std::{
    io::{BufWriter, Write},
    vec,
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT,
        USED_NONCES_SEED, VAULT_SEED,
    },
    error::ErrorCode,
    state::{config::Config, used_nonces::UsedNonces},
    FinalizeDepositData,
};

use super::FinalizeDepositResponse;
use crate::instructions::wormhole_cpi::*;

#[derive(Accounts)]
#[instruction(data: FinalizeDepositData)]
pub struct FinalizeWithdraw<'info> {
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
        mut,
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    /// CHECK: this can be any type of account
    pub recipient: UncheckedAccount<'info>,

    #[account(
        address = Pubkey::from_str(&data.payload.token).or(err!(ErrorCode::SolanaTokenParsingFailed))?,
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

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
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = wormhole.payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
        token::token_program = token_program,
    )]
    pub token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub wormhole: WormholeCPI<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> FinalizeWithdraw<'info> {
    pub fn process(&mut self, data: FinalizeWithdrawData, wormhole_message_bump: u8) -> Result<()> {
        UsedNonces::use_nonce(
            data.payload.nonce,
            &self.used_nonces,
            &mut self.config,
            self.authority.to_account_info(),
            self.wormhole.payer.to_account_info(),
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
        }
        .try_to_vec()?;

        self.wormhole.post_message(payload, wormhole_message_bump)?;

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawPayload {
    pub nonce: u128,
    pub amount: u128,
    pub fee_recipient: Option<String>,
}

impl WithdrawPayload {
    fn serialize_for_signature(&self, recipient: &Pubkey, token: &Pubkey) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(vec![]);
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        token.to_string().serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.amount), &mut writer)?;
        writer.write(&[2])?;
        recipient.to_string().serialize(&mut writer)?;
        self.fee_recipient.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeWithdrawData {
    pub payload: WithdrawPayload,
    signature: [u8; 65],
}

impl FinalizeWithdrawData {
    pub fn verify_signature(
        &self,
        recipient: &Pubkey,
        token: &Pubkey,
        derived_near_bridge_address: &[u8; 64],
    ) -> Result<()> {
        let borsh_encoded = self.payload.serialize_for_signature(recipient, token)?;
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
pub struct FinalizeWithdrawResponse {
    pub nonce: u128,
}
