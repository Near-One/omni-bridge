use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

use crate::{
    constants::SOL_VAULT_SEED,
    error::ErrorCode,
    instructions::wormhole_cpi::*,
    state::message::{init_transfer::InitTransferPayload, Payload},
};

#[derive(Accounts)]
pub struct InitTransferSol<'info> {
    #[account(
        mut,
        seeds = [SOL_VAULT_SEED],
        bump = wormhole.config.bumps.sol_vault,
    )]
    pub sol_vault: SystemAccount<'info>,

    #[account(
        mut,
        owner = wormhole.system_program.key(),
    )]
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
            payload.native_fee.checked_add(payload.amount.try_into().unwrap())
                .unwrap(),
        )?;

        self.wormhole.post_message(payload.serialize_for_near((
            self.wormhole.sequence.sequence,
            self.user.key(),
            Pubkey::default(),
        ))?)?;

        Ok(())
    }
}
