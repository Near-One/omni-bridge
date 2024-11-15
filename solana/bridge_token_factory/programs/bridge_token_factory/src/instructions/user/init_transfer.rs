use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{burn, transfer_checked, Burn, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::ErrorCode;
use crate::{constants::WRAPPED_MINT_SEED, instructions::wormhole_cpi::*};
use crate::{
    constants::{AUTHORITY_SEED, VAULT_SEED},
    state::message::{init_transfer::InitTransferPayload, Payload},
};

#[derive(Accounts)]
pub struct InitTransfer<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
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
    pub vault: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub user: Signer<'info>,

    pub wormhole: WormholeCPI<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> InitTransfer<'info> {
    pub fn process(&self, token: Option<String>, payload: InitTransferPayload) -> Result<()> {
        if let Some(token) = token {
            // Bridged version. We have a proof of the mint address
            require!(self.vault.is_none(), ErrorCode::BridgedTokenHasVault);
            let (expected_mint_address, _) = Pubkey::find_program_address(
                &[WRAPPED_MINT_SEED, token.as_bytes().as_ref()],
                &crate::ID,
            );
            require_keys_eq!(
                self.mint.key(),
                expected_mint_address,
                ErrorCode::InvalidBridgedToken
            );
            require!(
                self.mint.mint_authority.contains(self.authority.key),
                ErrorCode::InvalidBridgedToken
            );

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
        } else {
            // Native version. We have a proof by vault existence
            if let Some(vault) = &self.vault {
                transfer_checked(
                    CpiContext::new(
                        self.token_program.to_account_info(),
                        TransferChecked {
                            from: self.from.to_account_info(),
                            to: vault.to_account_info(),
                            authority: self.user.to_account_info(),
                            mint: self.mint.to_account_info(),
                        },
                    ),
                    payload.amount.try_into().unwrap(),
                    self.mint.decimals,
                )?;
            } else {
                return err!(ErrorCode::NativeTokenHasNoVault);
            }
        }

        self.wormhole.post_message(payload.serialize_for_near((
            self.wormhole.sequence.sequence,
            self.user.key(),
            self.mint.key(),
        ))?)?;

        Ok(())
    }
}
