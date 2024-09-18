use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};
use anchor_spl::token_interface::{
    spl_pod::bytemuck, token_metadata_initialize, Mint, TokenInterface, TokenMetadataInitialize,
};

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
        mint::freeze_authority = mint,
        extensions::metadata_pointer::authority = mint,
        extensions::metadata_pointer::metadata_address = mint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> DeployToken<'info> {
    pub fn initialize_token_metadata(&self, metadata: MetadataPayload) -> Result<()> {
        let seed = metadata.token.as_bytes().as_ref();
        let (_, bump) = Pubkey::find_program_address(&[seed], &crate::ID);
        let signer_seeds = &[&[seed, bytemuck::pod_bytes_of(&bump)][..]];

        let cpi_accounts = TokenMetadataInitialize {
            token_program_id: self.token_program.to_account_info(),
            mint: self.mint.to_account_info(),
            metadata: self.mint.to_account_info(),
            mint_authority: self.mint.to_account_info(),
            update_authority: self.mint.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token_metadata_initialize(cpi_ctx, metadata.name, metadata.symbol, String::new())?;
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
