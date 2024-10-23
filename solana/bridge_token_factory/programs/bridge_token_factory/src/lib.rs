use anchor_lang::prelude::*;
use instructions::*;

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;
use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "mainnet")] {
        declare_id!("2ajXVaqXXpHWtPnW3tKZukuXHGGjVcENjuZaWrz6NhD4");
    } else if #[cfg(feature = "solana-devnet")] {
        declare_id!("BfXGzL2m8hFjVsYgzMMeE7wSNd8FAV1PPet81Qb7tgcT");
    }
}

#[program]
pub mod bridge_token_factory {
    use super::*;

    pub fn deploy_token(ctx: Context<DeployToken>, data: DeployTokenData) -> Result<()> {
        msg!("Deploying token");

        data.verify_signature()?;
        ctx.accounts
            .initialize_token_metadata(data.metadata, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn finalize_deposit(
        ctx: Context<FinalizeDeposit>,
        data: FinalizeDepositData,
    ) -> Result<()> {
        msg!("Finalizing deposit");

        data.verify_signature()?;
        ctx.accounts.mint(data, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn register_mint(
        ctx: Context<RegisterMint>,
        metadata_override: MetadataOverride,
    ) -> Result<()> {
        msg!("Registering mint");

        ctx.accounts
            .process(metadata_override, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn send(ctx: Context<Send>, data: SendData) -> Result<()> {
        msg!("Sending");

        ctx.accounts.process(data, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn finalize_withdraw(
        ctx: Context<FinalizeWithdraw>,
        data: FinalizeDepositData,
    ) -> Result<()> {
        msg!("Finalizing withdraw");

        data.verify_signature()?;
        ctx.accounts.process(data, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn repay(ctx: Context<Repay>, payload: RepayPayload) -> Result<()> {
        msg!("Repaying");

        ctx.accounts.process(payload, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }
}
