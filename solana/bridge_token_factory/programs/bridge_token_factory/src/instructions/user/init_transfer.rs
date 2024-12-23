use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};
use anchor_spl::{
    token_2022::{burn, transfer_checked, Burn, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{AUTHORITY_SEED, BRIDGE_TOKEN_CONFIG_SEED, SOL_VAULT_SEED, VAULT_SEED},
    error::ErrorCode,
    instructions::wormhole_cpi::*,
    state::{message::{init_transfer::InitTransferPayload, Payload}, token_config::TokenConfig},
};

#[derive(Accounts)]
pub struct InitTransfer<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        mut,
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

    #[account(
        mut,
        seeds = [SOL_VAULT_SEED],
        bump = wormhole.config.bumps.sol_vault,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [BRIDGE_TOKEN_CONFIG_SEED, mint.key().as_ref()],
        bump,
    )]
    pub bridge_token_config: Box<Account<'info, TokenConfig>>,

    #[account(
        mut,
        owner = wormhole.system_program.key(),
    )]
    pub user: Signer<'info>,

    pub wormhole: WormholeCPI<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> InitTransfer<'info> {
    pub fn process(&self, payload: &mut InitTransferPayload) -> Result<()> {
        if payload.native_fee > 0 {
            transfer(
                CpiContext::new(
                    self.wormhole.system_program.to_account_info(),
                    Transfer {
                        from: self.user.to_account_info(),
                        to: self.sol_vault.to_account_info(),
                    },
                ),
                payload.native_fee,
            )?;
        }

        if let Some(vault) = &self.vault {
            // Native version. We have a proof of token registration by vault existence
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
            // Bridged version. May be a fake token with our authority set but it will be ignored on the near side
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
        }

        let origin_decimals = self.bridge_token_config.origin_decimals;
        payload.fee = payload.fee * 10_u128.pow((origin_decimals - self.mint.decimals) as u32);
        payload.amount = payload.amount * 10_u128.pow((origin_decimals - self.mint.decimals) as u32);

        self.wormhole.post_message(payload.serialize_for_near((
            self.wormhole.sequence.sequence,
            self.user.key(),
            self.mint.key(),
        ))?)?;

        Ok(())
    }
}
