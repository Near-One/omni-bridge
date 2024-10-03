use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, VAULT_SEED},
    state::config::Config,
    FinalizeDepositData,
};

#[derive(Accounts)]
#[instruction(data: FinalizeDepositData)]
pub struct FinalizeWithdraw<'info> {
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
        constraint = recipient.key == &data.payload.recipient,
    )]
    /// CHECK: this can be any type of account
    pub recipient: AccountInfo<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = authority,
        seeds = [
            VAULT_SEED,
            mint.key().as_ref(),
        ],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

impl<'info> FinalizeWithdraw<'info> {
    pub fn process(&self, data: FinalizeDepositData) -> Result<()> {
        let bump = &[self.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];

        transfer(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                Transfer {
                    from: self.vault.to_account_info(),
                    to: self.recipient.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
                signer_seeds,
            ),
            data.payload.amount.try_into().unwrap(),
        )?;

        Ok(())
    }
}
