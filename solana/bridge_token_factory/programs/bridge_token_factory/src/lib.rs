use anchor_lang::prelude::*;
use instructions::*;

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use state::message::{
    deploy_token::DeployTokenPayload, finalize_transfer::FinalizeTransferPayload,
    init_transfer::InitTransferPayload, SignedPayload,
};

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

        data.verify_signature(
            (),
            &ctx.accounts.wormhole.config.derived_near_bridge_address,
        )?;
        ctx.accounts.initialize_token_metadata(data.payload)?;

        Ok(())
    }

    pub fn finalize_transfer_bridged(
        ctx: Context<FinalizeDepositBridged>,
        data: SignedPayload<FinalizeTransferPayload>,
    ) -> Result<()> {
        msg!("Finalizing transfer");

        data.verify_signature(
            (ctx.accounts.mint.key(), ctx.accounts.recipient.key()),
            &ctx.accounts.config.derived_near_bridge_address,
        )?;
        ctx.accounts.mint(data.payload)?;

        Ok(())
    }

    pub fn finalize_transfer_native(
        ctx: Context<FinalizeTransferNative>,
        data: SignedPayload<FinalizeTransferPayload>,
    ) -> Result<()> {
        msg!("Finalizing transfer");

        data.verify_signature(
            (ctx.accounts.mint.key(), ctx.accounts.recipient.key()),
            &ctx.accounts.config.derived_near_bridge_address,
        )?;
        ctx.accounts.process(data.payload)?;

        Ok(())
    }

    pub fn register_mint(
        ctx: Context<RegisterMint>,
        metadata_override: MetadataOverride,
    ) -> Result<()> {
        msg!("Registering mint");

        ctx.accounts.process(metadata_override)?;

        Ok(())
    }

    pub fn init_transfer_native(
        ctx: Context<InitTransferNative>,
        payload: InitTransferPayload,
    ) -> Result<()> {
        msg!("Initializing transfer");

        ctx.accounts.process(payload)?;

        Ok(())
    }

    pub fn init_transfer_bridged(
        ctx: Context<InitTransferBridged>,
        payload: InitTransferPayload,
    ) -> Result<()> {
        msg!("Initializing transfer");

        ctx.accounts.process(payload)?;

        Ok(())
    }
}
