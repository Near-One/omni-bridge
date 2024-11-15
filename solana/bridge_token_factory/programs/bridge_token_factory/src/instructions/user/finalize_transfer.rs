use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::{mint_to, transfer_checked, MintTo, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT,
        USED_NONCES_SEED, VAULT_SEED, WRAPPED_MINT_SEED,
    },
    state::{
        config::Config,
        message::{
            finalize_transfer::{FinalizeTransferPayload, FinalizeTransferResponse},
            Payload, SignedPayload,
        },
        used_nonces::UsedNonces,
    },
};

use crate::error::ErrorCode;
use crate::instructions::wormhole_cpi::*;

#[derive(Accounts)]
#[instruction(data: SignedPayload<FinalizeTransferPayload>)]
pub struct FinalizeTransfer<'info> {
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
    pub vault: Option<Box<InterfaceAccount<'info, TokenAccount>>>,

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

impl<'info> FinalizeTransfer<'info> {
    pub fn process(&mut self, token: Option<String>, data: FinalizeTransferPayload) -> Result<()> {
        UsedNonces::use_nonce(
            data.nonce,
            &self.used_nonces,
            &mut self.config,
            self.authority.to_account_info(),
            self.wormhole.payer.to_account_info(),
            &Rent::get()?,
            self.system_program.to_account_info(),
        )?;

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

            mint_to(
                CpiContext::new_with_signer(
                    self.token_program.to_account_info(),
                    MintTo {
                        mint: self.mint.to_account_info(),
                        to: self.token_account.to_account_info(),
                        authority: self.authority.to_account_info(),
                    },
                    &[&[AUTHORITY_SEED, &[self.config.bumps.authority]]],
                ),
                data.amount.try_into().unwrap(),
            )?;
        } else {
            // Native version. We have a proof by vault existence
            if let Some(vault) = &self.vault {
                transfer_checked(
                    CpiContext::new_with_signer(
                        self.token_program.to_account_info(),
                        TransferChecked {
                            from: vault.to_account_info(),
                            to: self.token_account.to_account_info(),
                            authority: self.authority.to_account_info(),
                            mint: self.mint.to_account_info(),
                        },
                        &[&[AUTHORITY_SEED, &[self.config.bumps.authority]]],
                    ),
                    data.amount.try_into().unwrap(),
                    self.mint.decimals,
                )?;
            } else {
                return err!(ErrorCode::NativeTokenHasNoVault);
            }
        }

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
