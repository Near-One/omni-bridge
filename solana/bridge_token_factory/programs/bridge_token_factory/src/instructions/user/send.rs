use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{AUTHORITY_SEED, VAULT_SEED};
use crate::instructions::wormhole_cpi::*;

#[derive(Accounts)]
pub struct Send<'info> {
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,

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

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(AnchorDeserialize, AnchorSerialize, Clone, Default)]
pub struct SendData {
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SendPayload {
    pub token: String,
    pub amount: u128,
    pub recipient: Pubkey,
    pub fee_recipient: Option<String>,
}

impl<'info> Send<'info> {
    pub fn process(&self, data: SendData, wormhole_message_bump: u8) -> Result<()> {
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
            data.amount.try_into().unwrap(),
            self.mint.decimals,
        )?;

        let payload = SendPayload {
            token: self.mint.key().to_string(),
            amount: data.amount,
            recipient: data.recipient,
            fee_recipient: data.fee_recipient,
        }
        .try_to_vec()?; // TODO: correct message payload

        self.wormhole.post_message(payload, wormhole_message_bump)?;

        Ok(())
    }
}
