use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction::transfer},
};
use deploy_token::*;
use finalize_deposit::*;

pub mod deploy_token;
pub mod finalize_deposit;

declare_id!("2ajXVaqXXpHWtPnW3tKZukuXHGGjVcENjuZaWrz6NhD4");

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

        update_account_lamports_to_minimum_balance(
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.signer.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        )?;

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
}

pub fn update_account_lamports_to_minimum_balance<'info>(
    account: AccountInfo<'info>,
    payer: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
) -> Result<()> {
    let extra_lamports = Rent::get()?.minimum_balance(account.data_len()) - account.get_lamports();
    if extra_lamports > 0 {
        invoke(
            &transfer(payer.key, account.key, extra_lamports),
            &[payer, account, system_program],
        )?;
    }
    Ok(())
}

#[error_code(offset = 6000)]
pub enum ErrorCode {
    #[msg("Invalid arguments")]
    InvalidArgs,
    #[msg("Signature verification failed")]
    SignatureVerificationFailed,
}
