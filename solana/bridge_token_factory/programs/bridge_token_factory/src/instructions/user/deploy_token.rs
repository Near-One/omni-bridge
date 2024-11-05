use crate::constants::{AUTHORITY_SEED, WRAPPED_MINT_SEED};
use crate::error::ErrorCode;
use crate::instructions::wormhole_cpi::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata as Metaplex, ID as MetaplexID,
};
use anchor_spl::token::{Mint, Token};

#[derive(Accounts)]
#[instruction(data: DeployTokenData)]
pub struct DeployToken<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,
    #[account(
        init,
        payer = wormhole.payer,
        seeds = [WRAPPED_MINT_SEED, data.metadata.token.as_bytes().as_ref()],
        bump,
        mint::decimals = data.metadata.decimals,
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

    pub wormhole: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
}

impl<'info> DeployToken<'info> {
    pub fn initialize_token_metadata(
        &self,
        metadata: MetadataPayload,
    ) -> Result<()> {
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
        .try_to_vec()?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MetadataPayload {
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DeployTokenData {
    pub metadata: MetadataPayload,
    signature: [u8; 65],
}

impl DeployTokenData {
    pub fn verify_signature(&self, derived_near_bridge_address: &[u8; 64]) -> Result<()> {
        let borsh_encoded =
            borsh::to_vec(&self.metadata).map_err(|_| error!(ErrorCode::InvalidArgs))?;
        let hash = keccak::hash(&borsh_encoded);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| error!(ErrorCode::SignatureVerificationFailed))?;

        require!(
            signer.0 == *derived_near_bridge_address,
            ErrorCode::SignatureVerificationFailed
        );

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DeployTokenResponse {
    pub token: String,
    pub solana_mint: Pubkey,
}
