use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{
    constants::{AUTHORITY_SEED, SOL_VAULT_SEED},
    error::ErrorCode,
    instructions::wormhole_cpi::*,
    state::message::{init_transfer::InitTransferPayload, Payload},
};

#[derive(Accounts)]
pub struct InitTransferSol<'info> {
    #[account(
        seeds = [AUTHORITY_SEED],
        bump = wormhole.config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [SOL_VAULT_SEED],
        bump = wormhole.config.bumps.sol_vault,
    )]
    pub sol_vault: SystemAccount<'info>,

    pub user: Signer<'info>,

    pub wormhole: WormholeCPI<'info>,
}

impl<'info> InitTransferSol<'info> {
    pub fn process(&self, payload: InitTransferPayload) -> Result<()> {
        require!(payload.fee == 0, ErrorCode::InvalidFee);

        transfer(
            CpiContext::new(
                self.wormhole.system_program.to_account_info(),
                Transfer {
                    from: self.user.to_account_info(),
                    to: self.sol_vault.to_account_info(),
                },
            ),
            payload.native_fee + (payload.amount as u64),
        )?;

        self.wormhole.post_message(payload.serialize_for_near((
            self.wormhole.sequence.sequence,
            self.user.key(),
            Pubkey::default(),
        ))?)?;

        Ok(())
    }
}
