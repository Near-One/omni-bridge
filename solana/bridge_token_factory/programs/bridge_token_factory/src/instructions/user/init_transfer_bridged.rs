use anchor_lang::prelude::*;
use anchor_spl::token::{burn, Burn, Mint, Token, TokenAccount};

use crate::constants::AUTHORITY_SEED;
use crate::instructions::wormhole_cpi::*;
use crate::state::message::init_transfer::InitTransferPayload;
use crate::state::message::Payload;

#[derive(Accounts)]
#[instruction(payload: InitTransferPayload)]
pub struct InitTransferBridged<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        mut,
        mint::authority = authority,
        mint::token_program = token_program,
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

impl<'info> InitTransferBridged<'info> {
    pub fn process(&self, payload: InitTransferPayload) -> Result<()> {
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

        self.wormhole.post_message(payload.serialize_for_near((
            self.wormhole.sequence.sequence,
            self.user.key(),
            self.mint.key(),
        ))?)?;

        Ok(())
    }
}
