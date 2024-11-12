use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT,
        USED_NONCES_SEED, VAULT_SEED,
    },
    state::{
        config::Config,
        message::{
            deposit::{DepositPayload, FinalizeDepositResponse}, Payload, SignedPayload
        },
        used_nonces::UsedNonces,
    },
};

use crate::instructions::wormhole_cpi::*;

#[derive(Accounts)]
#[instruction(data: SignedPayload<DepositPayload>)]
pub struct FinalizeWithdraw<'info> {
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
        constraint = !mint.mint_authority.contains(authority.key),
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
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

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

impl<'info> FinalizeWithdraw<'info> {
    pub fn process(&mut self, data: DepositPayload) -> Result<()> {
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

        transfer_checked(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                TransferChecked {
                    from: self.vault.to_account_info(),
                    to: self.token_account.to_account_info(),
                    authority: self.authority.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
                signer_seeds,
            ),
            data.amount.try_into().unwrap(),
            self.mint.decimals,
        )?;

        let payload = FinalizeDepositResponse {
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
