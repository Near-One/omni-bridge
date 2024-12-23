use crate::constants::{AUTHORITY_SEED, BRIDGE_TOKEN_CONFIG_SEED, WRAPPED_MINT_SEED};
use crate::instructions::wormhole_cpi::*;
use crate::state::message::SignedPayload;
use crate::state::message::{
    deploy_token::{DeployTokenPayload, DeployTokenResponse},
    Payload,
};
use crate::state::token_config::TokenConfig;
use anchor_lang::prelude::*;
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata as Metaplex, ID as MetaplexID,
};
use anchor_spl::token::{Mint, Token};

#[derive(Accounts)]
#[instruction(data: SignedPayload<DeployTokenPayload>)]
pub struct DeployToken<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,
    #[account(
        init,
        payer = wormhole.payer,
        seeds = [WRAPPED_MINT_SEED, data.payload.token.as_bytes().as_ref()],
        bump,
        mint::decimals = if data.payload.decimals > 9 { 9 } else { data.payload.decimals },
        mint::authority = authority,
    )]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [
            b"metadata",
            MetaplexID.as_ref(),
            &mint.key().to_bytes(),
        ],
        bump,
        seeds::program = MetaplexID,
    )]
    pub metadata: SystemAccount<'info>,
    #[account(
        init,
        payer = wormhole.payer,
        space = 8 + TokenConfig::INIT_SPACE,
        seeds = [BRIDGE_TOKEN_CONFIG_SEED, mint.key().as_ref()],
        bump,
    )]
    pub bridge_token_config: Account<'info, TokenConfig>,

    pub wormhole: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
}

impl<'info> DeployToken<'info> {
    pub fn initialize_token_metadata(&mut self, metadata: DeployTokenPayload) -> Result<()> {
        self.bridge_token_config.set_inner(TokenConfig {
            origin_decimals: metadata.decimals,
            dust: 0,
        });

        let bump = &[self.wormhole.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];

        let cpi_accounts = CreateMetadataAccountsV3 {
            payer: self.wormhole.payer.to_account_info(),
            update_authority: self.authority.to_account_info(),
            mint: self.mint.to_account_info(),
            metadata: self.metadata.to_account_info(),
            mint_authority: self.authority.to_account_info(),
            system_program: self.system_program.to_account_info(),
            rent: self.wormhole.rent.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        create_metadata_accounts_v3(
            cpi_ctx,
            DataV2 {
                name: metadata.name,
                symbol: metadata.symbol,
                uri: String::new(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            true, // TODO: Maybe better to make it immutable
            true,
            None,
        )?;

        let payload = DeployTokenResponse {
            token: metadata.token,
            solana_mint: self.mint.key(),
        }
        .serialize_for_near(())?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}
