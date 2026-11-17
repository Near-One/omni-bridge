use anchor_lang::prelude::*;
use instructions::{
    ApplyForTrustedRelayer, ChangeConfig, DeployToken, FinalizeTransfer, FinalizeTransferSol,
    GetRelayers, GetVersion, GrantTrustedRelayer, InitRelayerList, InitTransfer, InitTransferSol,
    Initialize, LogMetadata, Pause, RejectRelayerApplication, ResignTrustedRelayer,
    SetRelayerManager, UpdateMetadata, __client_accounts_apply_for_trusted_relayer,
    __client_accounts_change_config,
    __client_accounts_deploy_token, __client_accounts_finalize_transfer,
    __client_accounts_finalize_transfer_sol, __client_accounts_get_relayers,
    __client_accounts_get_version, __client_accounts_grant_trusted_relayer,
    __client_accounts_init_relayer_list, __client_accounts_init_transfer,
    __client_accounts_init_transfer_sol, __client_accounts_initialize,
    __client_accounts_log_metadata, __client_accounts_pause,
    __client_accounts_reject_relayer_application, __client_accounts_resign_trusted_relayer,
    __client_accounts_set_relayer_manager, __client_accounts_update_metadata,
};
use state::{
    message::{
        deploy_token::DeployTokenPayload, finalize_transfer::FinalizeTransferPayload,
        init_transfer::InitTransferPayload, SignedPayload,
    },
    relayer::RelayerEntry,
};

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

include!(concat!(env!("OUT_DIR"), "/program_id.rs"));

#[program]
#[allow(clippy::needless_pass_by_value)]
pub mod bridge_token_factory {
    use crate::error;
    use anchor_lang::require;

    use super::constants::{FINALIZE_TRANSFER_PAUSED, INIT_TRANSFER_PAUSED};
    use super::{
        msg, ApplyForTrustedRelayer, ChangeConfig, Clock, Context, DeployToken,
        DeployTokenPayload, FinalizeTransfer, FinalizeTransferPayload, FinalizeTransferSol,
        GetRelayers, GetVersion, GrantTrustedRelayer, InitRelayerList, InitTransfer,
        InitTransferPayload, InitTransferSol, Initialize, Key, LogMetadata, Pause, Pubkey,
        RejectRelayerApplication, RelayerEntry, ResignTrustedRelayer, Result, SetRelayerManager,
        SignedPayload, SolanaSysvar, UpdateMetadata,
    };

    pub fn initialize(
        ctx: Context<Initialize>,
        admin: Pubkey,
        pausable_admin: Pubkey,
        metadata_admin: Pubkey,
        derived_near_bridge_address: [u8; 64],
        relayer_stake_required: u64,
        relayer_waiting_period: i64,
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
            relayer_stake_required,
            relayer_waiting_period,
        )?;

        Ok(())
    }

    pub fn get_version(_ctx: Context<GetVersion>) -> Result<String> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
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
        require!(
            ctx.accounts.relayer_state.is_active(Clock::get()?.unix_timestamp),
            error::ErrorCode::RelayerNotActive
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
        require!(
            ctx.accounts.relayer_state.is_active(Clock::get()?.unix_timestamp),
            error::ErrorCode::RelayerNotActive
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

    pub fn set_derived_near_bridge_address(
        ctx: Context<ChangeConfig>,
        derived_near_bridge_address: [u8; 64],
    ) -> Result<()> {
        msg!("Setting derived NEAR bridge address");

        ctx.accounts
            .set_derived_near_bridge_address(derived_near_bridge_address)?;

        Ok(())
    }

    pub fn set_relayer_config(
        ctx: Context<ChangeConfig>,
        stake_required: u64,
        waiting_period: i64,
    ) -> Result<()> {
        msg!("Setting relayer config");

        ctx.accounts.set_relayer_config(stake_required, waiting_period)?;

        Ok(())
    }

    pub fn init_relayer_list(ctx: Context<InitRelayerList>) -> Result<()> {
        msg!("Initializing relayer list");

        ctx.accounts.process(ctx.bumps.relayer_list);

        Ok(())
    }

    pub fn set_relayer_manager(ctx: Context<SetRelayerManager>, manager: Pubkey) -> Result<()> {
        msg!("Setting relayer manager {}", manager);

        ctx.accounts.process(manager);

        Ok(())
    }

    pub fn apply_for_trusted_relayer(ctx: Context<ApplyForTrustedRelayer>) -> Result<()> {
        msg!("Applying for trusted relayer");

        ctx.accounts.process(ctx.bumps.relayer_state)?;

        Ok(())
    }

    pub fn resign_trusted_relayer(ctx: Context<ResignTrustedRelayer>) -> Result<()> {
        msg!("Resigning trusted relayer");

        ctx.accounts.process()?;

        Ok(())
    }

    pub fn grant_trusted_relayer(ctx: Context<GrantTrustedRelayer>, relayer: Pubkey) -> Result<()> {
        msg!("Granting trusted relayer {}", relayer);

        ctx.accounts.process(relayer, ctx.bumps.relayer_state)?;

        Ok(())
    }

    pub fn reject_relayer_application(
        ctx: Context<RejectRelayerApplication>,
        relayer: Pubkey,
    ) -> Result<()> {
        msg!("Rejecting relayer {}", relayer);

        ctx.accounts.process(&relayer);

        Ok(())
    }

    pub fn get_active_relayers(
        ctx: Context<GetRelayers>,
        from_index: u32,
        limit: u32,
    ) -> Result<Vec<RelayerEntry>> {
        ctx.accounts.relayer_list.page(true, from_index, limit)
    }

    pub fn get_pending_relayers(
        ctx: Context<GetRelayers>,
        from_index: u32,
        limit: u32,
    ) -> Result<Vec<RelayerEntry>> {
        ctx.accounts.relayer_list.page(false, from_index, limit)
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
