use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::MetadataAccount as MplMetadata,
    token_2022::{
        self,
        spl_token_2022::{
            self,
            extension::{
                metadata_pointer::MetadataPointer, BaseStateWithExtensions, StateWithExtensions,
            },
        },
    },
    token_interface::{
        spl_token_metadata_interface::state::TokenMetadata, Mint, TokenAccount, TokenInterface,
    },
};

use crate::instructions::wormhole_cpi::*;
use crate::{
    constants::{AUTHORITY_SEED, VAULT_SEED},
    state::message::Payload,
};
use crate::{error::ErrorCode, state::message::log_metadata::LogMetadataPayload};
use anchor_spl::metadata::ID as MetaplexID;

#[derive(Accounts)]
pub struct LogMetadata<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    pub override_authority: Option<Signer<'info>>,

    #[account()]
    pub metadata: Option<Account<'info, MplMetadata>>,

    #[account(
        init_if_needed,
        payer = wormhole.payer,
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

    pub wormhole: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MetadataOverride {
    pub name: String,
    pub symbol: String,
}

impl<'info> LogMetadata<'info> {
    pub fn process(&mut self, metadata_override: MetadataOverride) -> Result<()> {
        let (name, symbol) = if let Some(override_authority) = self.override_authority.as_ref() {
            match override_authority.key() {
                a if a == self.wormhole.config.admin => {}
                a if self.mint.mint_authority.contains(&a) => {}
                _ => return err!(ErrorCode::Unauthorized),
            }
            (metadata_override.name, metadata_override.symbol)
        } else {
            if self.token_program.key() == token_2022::ID {
                let mint_account_info = self.mint.to_account_info();
                let mint_data = mint_account_info.try_borrow_data()?;
                let mint_with_extension =
                    StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
                let metadata_pointer = mint_with_extension
                    .get_extension::<MetadataPointer>()
                    .or(err!(ErrorCode::TokenMetadataNotProvided))?;
                if metadata_pointer.metadata_address.0 == self.mint.key() {
                    let metadata =
                        mint_with_extension.get_variable_len_extension::<TokenMetadata>()?;
                    (metadata.name, metadata.symbol)
                } else {
                    let metadata = self
                        .metadata
                        .as_ref()
                        .ok_or(error!(ErrorCode::TokenMetadataNotProvided))?;
                    require_keys_eq!(metadata.key(), metadata_pointer.metadata_address.0);
                    (metadata.name.clone(), metadata.symbol.clone())
                }
            } else {
                let metadata = self
                    .metadata
                    .as_ref()
                    .ok_or(error!(ErrorCode::TokenMetadataNotProvided))?;
                require_keys_eq!(
                    metadata.key(),
                    Pubkey::find_program_address(
                        &[
                            b"metadata",
                            MetaplexID.as_ref(),
                            &self.mint.key().to_bytes()
                        ],
                        &MetaplexID
                    )
                    .0
                );
                (metadata.name.clone(), metadata.symbol.clone())
            }
        };

        let payload = LogMetadataPayload {
            token: self.mint.key(),
            name,
            symbol,
            decimals: self.mint.decimals,
        }
        .serialize_for_near(())?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}
