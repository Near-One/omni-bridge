use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{
    constants::SOL_VAULT_SEED,
    error::ErrorCode,
    instructions::wormhole_cpi::{
        WormholeCPI, WormholeCPIBumps, __client_accounts_wormhole_cpi,
        __cpi_client_accounts_wormhole_cpi,
    },
    state::message::{init_transfer::InitTransferPayload, Payload},
};

#[derive(Accounts)]
pub struct InitTransferSol<'info> {
    #[account(
        mut,
        seeds = [SOL_VAULT_SEED],
        bump = common.config.bumps.sol_vault,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(
        mut,
        owner = common.system_program.key(),
    )]
    pub user: Signer<'info>,

    pub common: WormholeCPI<'info>,
}

impl InitTransferSol<'_> {
    pub fn process(&self, payload: &InitTransferPayload) -> Result<()> {
        require!(payload.fee == 0, ErrorCode::InvalidFee);
        require!(payload.amount > 0, ErrorCode::InvalidArgs);

        transfer(
            CpiContext::new(
                self.common.system_program.to_account_info(),
                Transfer {
                    from: self.user.to_account_info(),
                    to: self.sol_vault.to_account_info(),
                },
            ),
            payload
                .native_fee
                .checked_add(
                    payload.amount.try_into().map_err(|_| error!(ErrorCode::InvalidArgs))?,
                )
                .ok_or_else(|| error!(ErrorCode::InvalidArgs))?,
        )?;

        self.common.post_message(payload.serialize_for_near((
            self.common.sequence.sequence,
            self.user.key(),
            Pubkey::default(),
        ))?)?;

        Ok(())
    }
}
