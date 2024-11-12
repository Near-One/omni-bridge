use crate::{
    instructions::wormhole_cpi::*,
    state::message::{
        finalize_transfer::{FinalizeTransferPayload, FinalizeTransferResponse},
        Payload, SignedPayload,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT,
        USED_NONCES_SEED,
    },
    state::{config::Config, used_nonces::UsedNonces},
};

#[derive(Accounts)]
#[instruction(data: SignedPayload<FinalizeTransferPayload>)]
pub struct FinalizeDepositBridged<'info> {
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

    /// CHECK: this can be any type of account
    pub recipient: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,
    #[account(
        mut,
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

impl<'info> FinalizeDepositBridged<'info> {
    pub fn mint(&mut self, data: FinalizeTransferPayload) -> Result<()> {
        UsedNonces::use_nonce(
            data.nonce,
            &self.used_nonces,
            &mut self.config,
            self.authority.to_account_info(),
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
        mint_to(cpi_ctx, data.amount.try_into().unwrap())?;

        let payload = FinalizeTransferResponse {
            token: self.mint.key(),
            amount: data.amount,
            fee_recipient: data.fee_recipient.unwrap_or_default(),
            nonce: data.nonce,
        }
        .serialize_for_near(())?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}
