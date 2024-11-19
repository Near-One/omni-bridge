use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, SOL_VAULT_SEED, USED_NONCES_ACCOUNT_SIZE,
        USED_NONCES_PER_ACCOUNT, USED_NONCES_SEED,
    },
    instructions::wormhole_cpi::*,
    state::{
        config::Config,
        message::{
            finalize_transfer::{FinalizeTransferPayload, FinalizeTransferResponse},
            Payload, SignedPayload,
        },
        used_nonces::UsedNonces,
    },
};
use anchor_lang::{
    prelude::*,
    system_program::{transfer, Transfer},
};

#[derive(Accounts)]
#[instruction(data: SignedPayload<FinalizeTransferPayload>)]
pub struct FinalizeTransferSol<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bumps.config,
    )]
    pub config: Box<Account<'info, Config>>,
    #[account(
        init_if_needed,
        space = USED_NONCES_ACCOUNT_SIZE as usize,
        payer = wormhole.payer,
        seeds = [
            USED_NONCES_SEED,
            &(data.payload.destination_nonce / USED_NONCES_PER_ACCOUNT as u64).to_le_bytes(),
        ],
        bump,
    )]
    pub used_nonces: AccountLoader<'info, UsedNonces>,
    #[account(
        mut,
        seeds = [AUTHORITY_SEED],
        bump = config.bumps.authority,
    )]
    pub authority: SystemAccount<'info>,

    #[account(mut)]
    pub recipient: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [SOL_VAULT_SEED],
        bump = config.bumps.sol_vault,
    )]
    pub sol_vault: SystemAccount<'info>,

    pub wormhole: WormholeCPI<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> FinalizeTransferSol<'info> {
    pub fn process(&mut self, data: FinalizeTransferPayload) -> Result<()> {
        UsedNonces::use_nonce(
            data.destination_nonce,
            &self.used_nonces,
            &mut self.config,
            self.authority.to_account_info(),
            self.wormhole.payer.to_account_info(),
            &Rent::get()?,
            self.system_program.to_account_info(),
        )?;

        transfer(
            CpiContext::new_with_signer(
                self.wormhole.system_program.to_account_info(),
                Transfer {
                    from: self.sol_vault.to_account_info(),
                    to: self.recipient.to_account_info(),
                },
                &[&[SOL_VAULT_SEED, &[self.config.bumps.sol_vault]]],
            ),
            data.amount.try_into().unwrap(),
        )?;

        let payload = FinalizeTransferResponse {
            token: Pubkey::default(),
            amount: data.amount,
            fee_recipient: data.fee_recipient.unwrap_or_default(),
            transfer_id: data.transfer_id,
        }
        .serialize_for_near(())?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}
