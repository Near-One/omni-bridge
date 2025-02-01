use anchor_lang::prelude::*;
use instructions::{
    DeployToken, FinalizeTransfer, FinalizeTransferSol, InitTransfer, InitTransferSol, Initialize,
    LogMetadata, __client_accounts_deploy_token, __client_accounts_finalize_transfer,
    __client_accounts_finalize_transfer_sol, __client_accounts_init_transfer,
    __client_accounts_init_transfer_sol, __client_accounts_initialize,
    __client_accounts_log_metadata,
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
pub mod bridge_token_factory {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        admin: Pubkey,
        derived_near_bridge_address: [u8; 64],
    ) -> Result<()> {
        msg!("Initializing");

        ctx.accounts.process(
            admin,
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
        msg!("Initializing transfer");

        ctx.accounts.process(&payload)?;

        Ok(())
    }

    pub fn init_transfer_sol(
        ctx: Context<InitTransferSol>,
        payload: InitTransferPayload,
    ) -> Result<()> {
        msg!("Initializing transfer");

        ctx.accounts.process(&payload)?;

        Ok(())
    }
}
