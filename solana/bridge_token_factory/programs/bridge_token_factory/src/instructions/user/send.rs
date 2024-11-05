use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::instructions::wormhole_cpi::*;
use crate::{
    constants::{AUTHORITY_SEED, VAULT_SEED},
    state::message::{send::SendPayload, Payload},
};

#[derive(Accounts)]
pub struct Send<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        token::mint = mint,
        token::token_program = token_program,
    )]
    pub from: Box<InterfaceAccount<'info, TokenAccount>>,
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
    pub user: Signer<'info>,

    pub wormhole: WormholeCPI<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> Send<'info> {
    pub fn process(&self, payload: SendPayload) -> Result<()> {
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
            payload.amount.try_into().unwrap(),
            self.mint.decimals,
        )?;

        self.wormhole
            .post_message(payload.serialize_for_near(self.mint.key())?)?;

        Ok(())
    }
}
