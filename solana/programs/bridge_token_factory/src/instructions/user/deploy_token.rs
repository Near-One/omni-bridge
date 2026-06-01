use crate::constants::{AUTHORITY_SEED, MAX_ALLOWED_DECIMALS, METADATA_SEED, WRAPPED_MINT_SEED};
use crate::instructions::wormhole_cpi::{
    WormholeCPI, WormholeCPIBumps, __client_accounts_wormhole_cpi,
    __cpi_client_accounts_wormhole_cpi,
};
use crate::state::message::SignedPayload;
use crate::state::message::{
    deploy_token::{DeployTokenPayload, DeployTokenResponse},
    Payload,
};
use anchor_lang::prelude::*;
use solana_program::hash::hash;
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata as Metaplex, ID as MetaplexID,
};
use anchor_spl::token::{Mint, Token};

pub trait StringExt {
    fn to_hashed_bytes(&self) -> [u8; 32];
}

impl StringExt for String {
    fn to_hashed_bytes(&self) -> [u8; 32] {
        let bytes = self.as_bytes();
        if bytes.len() > 32 {
            let hash = hash(bytes);
            hash.to_bytes()
        } else {
            let mut padded_bytes = [0u8; 32];
            padded_bytes[..bytes.len()].copy_from_slice(bytes);
            padded_bytes
        }
    }
}

#[derive(Accounts)]
#[instruction(data: SignedPayload<DeployTokenPayload>)]
pub struct DeployToken<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = common.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,
    #[account(
        init,
        payer = common.payer,
        seeds = [WRAPPED_MINT_SEED, data.payload.token.to_hashed_bytes().as_ref()],
        bump,
        mint::decimals = std::cmp::min(MAX_ALLOWED_DECIMALS, data.payload.decimals),
        mint::authority = authority,
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
    pub metadata: SystemAccount<'info>,

    pub common: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
}

impl DeployToken<'_> {
    pub fn initialize_token_metadata(&self, mut metadata: DeployTokenPayload) -> Result<()> {
        let bump = &[self.common.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];
        let origin_decimals = metadata.decimals;
        metadata.decimals = std::cmp::min(MAX_ALLOWED_DECIMALS, metadata.decimals);

        let cpi_accounts = CreateMetadataAccountsV3 {
            payer: self.common.payer.to_account_info(),
            update_authority: self.authority.to_account_info(),
            mint: self.mint.to_account_info(),
            metadata: self.metadata.to_account_info(),
            mint_authority: self.authority.to_account_info(),
            system_program: self.system_program.to_account_info(),
            rent: self.common.rent.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_metadata_program.to_account_info(),
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
            decimals: metadata.decimals,
            origin_decimals,
        }
        .serialize_for_near(())?;

        self.common.post_message(payload)?;

        Ok(())
    }
}
