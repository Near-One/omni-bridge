use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, CreateMetadataAccountsV3, Metadata as Metaplex, ID as MetaplexID,
};
use anchor_spl::token::{Mint, Token};
use wormhole_anchor_sdk::wormhole::program::Wormhole;
use wormhole_anchor_sdk::wormhole::{
    post_message, BridgeData, FeeCollector, Finality, PostMessage, SequenceTracker,
};

use crate::constants::{AUTHORITY_SEED, CONFIG_SEED, DERIVED_NEAR_BRIDGE_ADDRESS, MESSAGE_SEED};
use crate::state::config::Config;
use crate::error::ErrorCode;

#[derive(Accounts)]
#[instruction(data: DeployTokenData)]
pub struct DeployToken<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        seeds = [data.metadata.token.as_bytes().as_ref()],
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

    /// Wormhole bridge data. [`wormhole::post_message`] requires this account
    /// be mutable.
    #[account(
        mut,
        address = config.wormhole.bridge,
    )]
    pub wormhole_bridge: Box<Account<'info, BridgeData>>,

    /// Wormhole fee collector. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.fee_collector
    )]
    pub wormhole_fee_collector: Box<Account<'info, FeeCollector>>,

    /// Emitter's sequence account. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.sequence
    )]
    pub wormhole_sequence: Box<Account<'info, SequenceTracker>>,

    /// CHECK: Wormhole Message. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        seeds = [
            MESSAGE_SEED,
            &wormhole_sequence.next_value().to_le_bytes()[..]
        ],
        bump,
    )]
    pub wormhole_message: SystemAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub token_metadata_program: Program<'info, Metaplex>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> DeployToken<'info> {
    pub fn initialize_token_metadata(
        &self,
        metadata: MetadataPayload,
        wormhole_message_bump: u8,
    ) -> Result<()> {
        let bump = &[self.config.bumps.authority];
        let signer_seeds = &[&[AUTHORITY_SEED, bump][..]];

        let cpi_accounts = CreateMetadataAccountsV3 {
            payer: self.payer.to_account_info(),
            update_authority: self.authority.to_account_info(),
            mint: self.mint.to_account_info(),
            metadata: self.metadata.to_account_info(),
            mint_authority: self.authority.to_account_info(),
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
        }.try_to_vec()?;

        post_message(
            CpiContext::new_with_signer(
                self.wormhole_program.to_account_info(),
                PostMessage {
                    config: self.wormhole_bridge.to_account_info(),
                    message: self.wormhole_message.to_account_info(),
                    emitter: self.config.to_account_info(),
                    sequence: self.wormhole_sequence.to_account_info(),
                    payer: self.payer.to_account_info(),
                    fee_collector: self.wormhole_fee_collector.to_account_info(),
                    clock: self.clock.to_account_info(),
                    rent: self.rent.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                },
                &[
                    &[
                        MESSAGE_SEED,
                        &self.wormhole_sequence.next_value().to_le_bytes()[..],
                        &[wormhole_message_bump],
                    ],
                    &[CONFIG_SEED, &[self.config.bumps.config]], // emitter
                ],
            ),
            0,
            payload,
            Finality::Finalized,
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
            borsh::to_vec(&self.metadata).map_err(|_| error!(ErrorCode::InvalidArgs))?;
        let hash = keccak::hash(&borsh_encoded);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| error!(ErrorCode::SignatureVerificationFailed))?;

        require!(
            signer.0 == DERIVED_NEAR_BRIDGE_ADDRESS,
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
