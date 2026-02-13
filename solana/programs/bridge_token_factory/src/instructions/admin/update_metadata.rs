use anchor_lang::prelude::*;
use anchor_lang::{
    prelude::{Interface, SystemAccount},
    Accounts, Key,
};
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{
    update_metadata_accounts_v2, Metadata, MetadataAccount, UpdateMetadataAccountsV2,
};
use anchor_spl::{metadata::ID as MetaplexID, token::Mint, token_interface::TokenInterface};

use crate::constants::{AUTHORITY_SEED, CONFIG_SEED, METADATA_SEED};
use crate::state::config::Config;

#[derive(Accounts)]
pub struct UpdateMetadata<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        mint::token_program = token_program,
    )]
    pub mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [
            METADATA_SEED,
            MetaplexID.as_ref(),
            &mint.key().to_bytes(),
        ],
        bump,
        seeds::program = MetaplexID,
    )]
    pub metadata: Box<Account<'info, MetadataAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub token_metadata_program: Program<'info, Metadata>,

    #[account(
        mut,
        constraint = signer.key() == config.metadata_admin || signer.key() == config.admin @
            crate::error::ErrorCode::Unauthorized,
    )]
    pub signer: Signer<'info>,
}

impl UpdateMetadata<'_> {
    pub fn process(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        uri: Option<String>,
    ) -> Result<()> {
        let bump = &[self.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];
        let cpi_accounts = UpdateMetadataAccountsV2 {
            metadata: self.metadata.to_account_info(),
            update_authority: self.authority.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );

        update_metadata_accounts_v2(
            cpi_ctx,
            None,
            Some(DataV2 {
                name: name.unwrap_or_else(|| self.metadata.name.clone()),
                symbol: symbol.unwrap_or_else(|| self.metadata.symbol.clone()),
                uri: uri.unwrap_or_else(|| self.metadata.uri.clone()),
                seller_fee_basis_points: self.metadata.seller_fee_basis_points,
                creators: self.metadata.creators.clone(),
                collection: self.metadata.collection.clone(),
                uses: self.metadata.uses.clone(),
            }),
            None,
            None,
        )?;

        Ok(())
    }
}
