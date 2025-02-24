use anchor_lang::prelude::*;
use instructions::{
    ChangeConfig, DeployToken, FinalizeTransfer, FinalizeTransferSol, InitTransfer,
    InitTransferSol, Initialize, LogMetadata, Pause, UpdateMetadata,
    __client_accounts_change_config, __client_accounts_deploy_token,
    __client_accounts_finalize_transfer, __client_accounts_finalize_transfer_sol,
    __client_accounts_init_transfer, __client_accounts_init_transfer_sol,
    __client_accounts_initialize, __client_accounts_log_metadata, __client_accounts_pause,
    __client_accounts_update_metadata,
};
use state::message::{
    deploy_token::DeployTokenPayload, finalize_transfer::FinalizeTransferPayload,
    init_transfer::InitTransferPayload, SignedPayload,
};

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

include!(concat!(env!("OUT_DIR"), "/program_id.rs"));

#[program]
#[allow(clippy::needless_pass_by_value)]
pub mod omni_bridge {
    use crate::error;
    use anchor_lang::require;

    use super::constants::{FINALIZE_TRANSFER_PAUSED, INIT_TRANSFER_PAUSED};
    use super::{
        msg, ChangeConfig, Context, DeployToken, DeployTokenPayload, FinalizeTransfer,
        FinalizeTransferPayload, FinalizeTransferSol, InitTransfer, InitTransferPayload,
        InitTransferSol, Initialize, Key, LogMetadata, Pause, Pubkey, Result, SignedPayload,
        UpdateMetadata,
    };

    pub fn initialize(
        ctx: Context<Initialize>,
        admin: Pubkey,
        pausable_admin: Pubkey,
        metadata_admin: Pubkey,
        derived_near_bridge_address: [u8; 64],
    ) -> Result<()> {
        msg!("Initializing");

        ctx.accounts.process(
            admin,
            pausable_admin,
            metadata_admin,
            derived_near_bridge_address,
            ctx.bumps.config,
            ctx.bumps.authority,
            ctx.bumps.sol_vault,
            ctx.bumps.wormhole_bridge,
            ctx.bumps.wormhole_fee_collector,
            ctx.bumps.wormhole_sequence,
        )?;

        Ok(())
    }

    pub fn deploy_token(
        ctx: Context<DeployToken>,
        data: SignedPayload<DeployTokenPayload>,
    ) -> Result<()> {
        msg!("Deploying token");

        data.verify_signature((), &ctx.accounts.common.config.derived_near_bridge_address)?;
        ctx.accounts.initialize_token_metadata(data.payload)?;

        Ok(())
    }

    pub fn finalize_transfer(
        ctx: Context<FinalizeTransfer>,
        data: SignedPayload<FinalizeTransferPayload>,
    ) -> Result<()> {
        require!(
            ctx.accounts.common.config.paused & FINALIZE_TRANSFER_PAUSED == 0,
            error::ErrorCode::Paused
        );
        msg!("Finalizing transfer");

        data.verify_signature(
            (ctx.accounts.mint.key(), ctx.accounts.recipient.key()),
            &ctx.accounts.common.config.derived_near_bridge_address,
        )?;
        ctx.accounts.process(data.payload)?;

        Ok(())
    }

    pub fn finalize_transfer_sol(
        ctx: Context<FinalizeTransferSol>,
        data: SignedPayload<FinalizeTransferPayload>,
    ) -> Result<()> {
        require!(
            ctx.accounts.common.config.paused & FINALIZE_TRANSFER_PAUSED == 0,
            error::ErrorCode::Paused
        );
        msg!("Finalizing transfer");

        data.verify_signature(
            (Pubkey::default(), ctx.accounts.recipient.key()),
            &ctx.accounts.config.derived_near_bridge_address,
        )?;
        ctx.accounts.process(data.payload)?;

        Ok(())
    }

    pub fn log_metadata(ctx: Context<LogMetadata>) -> Result<()> {
        msg!("Logging metadata");

        ctx.accounts.process()?;

        Ok(())
    }

    pub fn init_transfer(ctx: Context<InitTransfer>, payload: InitTransferPayload) -> Result<()> {
        require!(
            ctx.accounts.common.config.paused & INIT_TRANSFER_PAUSED == 0,
            error::ErrorCode::Paused
        );
        msg!("Initializing transfer");

        ctx.accounts.process(&payload)?;

        Ok(())
    }

    pub fn init_transfer_sol(
        ctx: Context<InitTransferSol>,
        payload: InitTransferPayload,
    ) -> Result<()> {
        require!(
            ctx.accounts.common.config.paused & INIT_TRANSFER_PAUSED == 0,
            error::ErrorCode::Paused
        );
        msg!("Initializing transfer");

        ctx.accounts.process(&payload)?;

        Ok(())
    }

    pub fn pause(ctx: Context<Pause>) -> Result<()> {
        msg!("Pausing");

        ctx.accounts.process()?;

        Ok(())
    }

    pub fn unpause(ctx: Context<ChangeConfig>, paused: u8) -> Result<()> {
        msg!("Unpausing");

        ctx.accounts.set_paused(paused)?;

        Ok(())
    }

    pub fn set_admin(ctx: Context<ChangeConfig>, admin: Pubkey) -> Result<()> {
        msg!("Setting admin");

        ctx.accounts.set_admin(admin)?;

        Ok(())
    }

    pub fn set_pausable_admin(ctx: Context<ChangeConfig>, pausable_admin: Pubkey) -> Result<()> {
        msg!("Setting pausable admin");

        ctx.accounts.set_pausable_admin(pausable_admin)?;

        Ok(())
    }

    pub fn set_metadata_admin(ctx: Context<ChangeConfig>, metadata_admin: Pubkey) -> Result<()> {
        msg!("Setting metadata admin");

        ctx.accounts.set_metadata_admin(metadata_admin)?;

        Ok(())
    }

    pub fn update_metadata(
        ctx: Context<UpdateMetadata>,
        name: Option<String>,
        symbol: Option<String>,
        uri: Option<String>,
    ) -> Result<()> {
        msg!("Updating metadata");

        ctx.accounts.process(name, symbol, uri)?;

        Ok(())
    }
}
