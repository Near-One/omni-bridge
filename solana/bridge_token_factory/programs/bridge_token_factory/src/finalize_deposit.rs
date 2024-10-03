use anchor_lang::{
    prelude::*,
    solana_program::{keccak, secp256k1_recover::secp256k1_recover},
};
use anchor_spl::{
    associated_token::AssociatedToken, token::{Mint, Token, TokenAccount, mint_to, MintTo}
};
use near_sdk::json_types::U128;
use std::{
    io::{BufWriter, Write},
    vec,
};

#[derive(Accounts)]
#[instruction(data: FinalizeDepositData)]
pub struct FinalizeDeposit<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        constraint = recipient.key == &data.payload.recipient,
    )]
    /// CHECK: this can be any type of account
    pub recipient: AccountInfo<'info>,
    #[account(
        mut,
        seeds = [data.payload.token.as_bytes().as_ref()],
        bump,
    )]
    pub mint: Account<'info, Mint>,
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub token_account: Account<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

impl<'info> FinalizeDeposit<'info> {
    pub fn mint(&self, data: FinalizeDepositData, mint_bump: u8) -> Result<()> {
        let seed = data.payload.token.as_bytes().as_ref();
        let bump = &[mint_bump];
        let signer_seeds = &[&[seed, bump][..]];

        let cpi_accounts = MintTo {
            mint: self.mint.to_account_info(),
            to: self.token_account.to_account_info(),
            authority: self.mint.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        mint_to(cpi_ctx, data.payload.amount.try_into().unwrap())?;

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
