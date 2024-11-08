use anchor_lang::prelude::*;
use anchor_spl::token::{burn, Burn, Mint, Token, TokenAccount};

use crate::constants::{AUTHORITY_SEED, WRAPPED_MINT_SEED};
use crate::instructions::wormhole_cpi::*;
use crate::state::message::repay::RepayPayload;
use crate::state::message::Payload;

#[derive(Accounts)]
#[instruction(payload: RepayPayload)]
pub struct Repay<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [WRAPPED_MINT_SEED, payload.token.as_bytes().as_ref()],
        bump,
        mint::authority = authority,
    )]
    pub mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        token::mint = mint,
    )]
    pub from: Box<Account<'info, TokenAccount>>,
    pub user: Signer<'info>,

    pub wormhole: WormholeCPI<'info>,

    pub token_program: Program<'info, Token>,
}

impl<'info> Repay<'info> {
    pub fn process(&self, payload: RepayPayload) -> Result<()> {
        burn(
            CpiContext::new(
                self.token_program.to_account_info(),
                Burn {
                    mint: self.mint.to_account_info(),
                    from: self.from.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            payload.amount.try_into().unwrap(),
        )?;

        self.wormhole
            .post_message(payload.serialize_for_near(())?)?;

        Ok(())
    }
}
