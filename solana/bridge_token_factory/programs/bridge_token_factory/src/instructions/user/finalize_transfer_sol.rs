use anchor_lang::{prelude::*, system_program::{transfer, Transfer}};
use crate::{
    constants::{
        AUTHORITY_SEED, CONFIG_SEED, SOL_VAULT_SEED, USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT, USED_NONCES_SEED,
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
            &(data.payload.nonce / USED_NONCES_PER_ACCOUNT as u128).to_le_bytes(),
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
            data.nonce,
            &self.used_nonces,
            &mut self.config,
            self.authority.to_account_info(),
            self.wormhole.payer.to_account_info(),
            &Rent::get()?,
            self.system_program.to_account_info(),
        )?;

        transfer(
            CpiContext::new(
                self.wormhole.system_program.to_account_info(),
                Transfer {
                    from: self.sol_vault.to_account_info(),
                    to: self.recipient.to_account_info(),
                },
            ),
            data.amount.try_into().unwrap(),
        )?;

        let payload = FinalizeTransferResponse {
            token: Pubkey::default(),
            amount: data.amount,
            fee_recipient: data.fee_recipient.unwrap_or_default(),
            nonce: data.nonce,
        }
        .serialize_for_near(())?;

        self.wormhole.post_message(payload)?;

        Ok(())
    }
}
