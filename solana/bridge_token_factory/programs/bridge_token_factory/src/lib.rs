use anchor_lang::prelude::*;
use instructions::*;

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

declare_id!("2ajXVaqXXpHWtPnW3tKZukuXHGGjVcENjuZaWrz6NhD4");

#[constant]
const DERIVED_NEAR_BRIDGE_ADDRESS: [u8; 64] = [
    251, 68, 120, 58, 81, 118, 152, 127, 82, 144, 201, 3, 155, 120, 205, 68, 127, 0, 13, 46, 181,
    138, 131, 83, 41, 60, 134, 18, 214, 185, 83, 102, 221, 254, 189, 217, 72, 147, 49, 87, 118,
    107, 41, 226, 91, 100, 139, 234, 44, 140, 74, 101, 135, 211, 213, 40, 231, 252, 77, 11, 96,
    209, 90, 183,
];

#[program]
pub mod bridge_token_factory {
    use super::*;

    pub fn deploy_token(ctx: Context<DeployToken>, data: DeployTokenData) -> Result<()> {
        msg!("Deploying token");

        data.verify_signature()?;
        ctx.accounts.initialize_token_metadata(data.metadata)?;

        // Emit event
        Ok(())
    }

    pub fn finalize_deposit(
        ctx: Context<FinalizeDeposit>,
        data: FinalizeDepositData,
    ) -> Result<()> {
        msg!("Finalizing deposit");

        data.verify_signature()?;
        ctx.accounts.mint(data)?;

        // Emit event
        Ok(())
    }

    pub fn register_mint(
        ctx: Context<RegisterMint>,
        name_override: String,
        symbol_override: String,
    ) -> Result<()> {
        msg!("Registering mint");

        ctx.accounts
            .process(name_override, symbol_override, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }

    pub fn send(ctx: Context<Send>, data: SendData) -> Result<()> {
        msg!("Omni transfer");

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
        ctx.accounts.process(data)?;

        // Emit event
        Ok(())
    }

    pub fn repay(ctx: Context<Repay>, payload: DepositPayload) -> Result<()> {
        msg!("Repaying");

        ctx.accounts.process(payload, ctx.bumps.wormhole_message)?;

        // Emit event
        Ok(())
    }
}

#[error_code(offset = 6000)]
pub enum ErrorCode {
    #[msg("Invalid arguments")]
    InvalidArgs,
    #[msg("Signature verification failed")]
    SignatureVerificationFailed,
}
