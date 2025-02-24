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

use crate::{
    constants::METADATA_SEED,
    instructions::wormhole_cpi::{
        WormholeCPI, WormholeCPIBumps, __client_accounts_wormhole_cpi,
        __cpi_client_accounts_wormhole_cpi,
    },
};
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
        bump = common.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: may be unitialized
    pub metadata: Option<UncheckedAccount<'info>>,

    #[account(
        init_if_needed,
        payer = common.payer,
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

    pub common: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl LogMetadata<'_> {
    fn parse_metadata_account(&self, address: Pubkey) -> Result<(String, String)> {
        let metadata = self
            .metadata
            .as_ref()
            .ok_or(error!(ErrorCode::TokenMetadataNotProvided))?
            .to_account_info();
        require_keys_eq!(
            metadata.key(),
            address,
            ErrorCode::InvalidTokenMetadataAddress,
        );
        if metadata.owner == &MetaplexID {
            let data = metadata.try_borrow_data()?;
            let metadata = MplMetadata::try_deserialize(&mut data.as_ref())?;
            Ok((metadata.name.clone(), metadata.symbol.clone()))
        } else {
            Ok((String::default(), String::default()))
        }
    }
    pub fn process(&mut self) -> Result<()> {
        let (name, symbol) = if self.token_program.key() == token_2022::ID {
            let mint_account_info = self.mint.to_account_info();
            let mint_data = mint_account_info.try_borrow_data()?;
            let mint_with_extension =
                StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

            if let Ok(metadata_pointer) = mint_with_extension.get_extension::<MetadataPointer>() {
                if metadata_pointer.metadata_address.0 == self.mint.key() {
                    // Embedded metadata
                    let metadata =
                        mint_with_extension.get_variable_len_extension::<TokenMetadata>()?;
                    (metadata.name, metadata.symbol)
                } else if metadata_pointer.metadata_address.0 != Pubkey::default() {
                    // Third-party metadata
                    self.parse_metadata_account(metadata_pointer.metadata_address.0)?
                } else {
                    // No metadata
                    (String::default(), String::default())
                }
            } else {
                // No metadata pointer extension found
                (String::default(), String::default())
            }
        } else {
            // Only metaplex is supported for the classic SPL tokens
            self.parse_metadata_account(
                Pubkey::find_program_address(
                    &[
                        METADATA_SEED,
                        MetaplexID.as_ref(),
                        &self.mint.key().to_bytes(),
                    ],
                    &MetaplexID,
                )
                .0,
            )?
        };

        let payload = LogMetadataPayload {
            token: self.mint.key(),
            name: name.trim_end_matches('\0').to_string(),
            symbol: symbol.trim_end_matches('\0').to_string(),
            decimals: self.mint.decimals,
        }
        .serialize_for_near(())?;

        self.common.post_message(payload)?;

        Ok(())
    }
}
