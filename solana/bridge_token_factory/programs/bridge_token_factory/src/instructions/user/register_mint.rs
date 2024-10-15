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
use wormhole_anchor_sdk::wormhole::{
    post_message, program::Wormhole, BridgeData, FeeCollector, Finality, PostMessage,
    SequenceTracker,
};

use super::MetadataPayload;
use crate::error::ErrorCode;
use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, MESSAGE_SEED, VAULT_SEED},
    state::config::Config,
};
use anchor_spl::metadata::ID as MetaplexID;

#[derive(Accounts)]
pub struct RegisterMint<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Account<'info, Config>,
    /// CHECK: PDA
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        constraint = !mint.mint_authority.contains(authority.key),
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    pub override_authority: Option<Signer<'info>>,

    #[account()]
    pub metadata: Option<Account<'info, MplMetadata>>,

    #[account(
        init,
        payer = payer,
        token::mint = mint,
        token::authority = authority,
        seeds = [
            VAULT_SEED,
            mint.key().as_ref(),
        ],
        bump,
        token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    /// Wormhole bridge data. [`wormhole::post_message`] requires this account
    /// be mutable.
    #[account(
        mut,
        address = config.wormhole.bridge,
    )]
    pub wormhole_bridge: Account<'info, BridgeData>,

    /// Wormhole fee collector. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.fee_collector
    )]
    pub wormhole_fee_collector: Account<'info, FeeCollector>,

    /// Emitter's sequence account. [`wormhole::post_message`] requires this
    /// account be mutable.
    #[account(
        mut,
        address = config.wormhole.sequence
    )]
    pub wormhole_sequence: Account<'info, SequenceTracker>,

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
    pub wormhole_message: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub wormhole_program: Program<'info, Wormhole>,
}

impl<'info> RegisterMint<'info> {
    pub fn process(
        &mut self,
        name_override: String,
        symbol_override: String,
        wormhole_message_bump: u8,
    ) -> Result<()> {
        let (name, symbol) = if let Some(override_authority) = self.override_authority.as_ref() {
            match override_authority.key() {
                a if a == self.config.admin => {}
                a if self.mint.mint_authority.contains(&a) => {}
                _ => return err!(ErrorCode::Unauthorized),
            }
            (name_override, symbol_override)
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

        // TODO: correct message payload
        let payload = MetadataPayload {
            token: self.mint.key().to_string(),
            name,
            symbol,
            decimals: self.mint.decimals,
        }
        .try_to_vec()?;

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
