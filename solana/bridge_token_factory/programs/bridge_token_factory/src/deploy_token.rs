use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata as Metaplex, ID as MetaplexID};
use anchor_spl::token::{Mint, Token};

#[derive(Accounts)]
#[instruction(data: DeployTokenData)]
pub struct DeployToken<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        payer = signer,
        seeds = [data.metadata.token.as_bytes().as_ref()],
        bump,
        mint::decimals = data.metadata.decimals,
        mint::authority = mint,
    )]
    pub mint: Account<'info, Mint>,
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

    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
}

impl<'info> DeployToken<'info> {
    pub fn initialize_token_metadata(&self, metadata: MetadataPayload, mint_bump: u8) -> Result<()> {
        let seed = metadata.token.as_bytes().as_ref();
        let bump = &[mint_bump];
        let signer_seeds = &[&[seed, bump][..]];

        let cpi_accounts = CreateMetadataAccountsV3 {
            payer: self.signer.to_account_info(),
            update_authority: self.mint.to_account_info(),
            mint: self.mint.to_account_info(),
            metadata: self.metadata.to_account_info(),
            mint_authority: self.mint.to_account_info(),
            system_program: self.system_program.to_account_info(),
            rent: self.rent.to_account_info(),
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
                uri:  String::new(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            true, // TODO: Maybe better to make it immutable
            true,
            None,
        )?;
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
    pub fn verify_signature(&self) -> Result<()> {
        let borsh_encoded =
            borsh::to_vec(&self.metadata).map_err(|_| crate::ErrorCode::InvalidArgs)?;
        let hash = keccak::hash(&borsh_encoded);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| crate::ErrorCode::SignatureVerificationFailed)?;

        require!(
            signer.0 == crate::DERIVED_NEAR_BRIDGE_ADDRESS,
            crate::ErrorCode::SignatureVerificationFailed
        );

        Ok(())
    }
}
