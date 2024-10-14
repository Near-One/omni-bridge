use anchor_lang::prelude::*;
use anchor_lang::system_program::transfer;
use anchor_lang::system_program::Transfer;
use bitvec::array::BitArray;

use crate::constants::{USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT};
use crate::error::ErrorCode;

use super::config::Config;

#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct UsedNonces {
    used: BitArray<[u8; (USED_NONCES_PER_ACCOUNT + 7) / 8]>,
}

impl UsedNonces {
    pub fn full_rent(rent: &Rent) -> u64 {
        rent.minimum_balance(USED_NONCES_ACCOUNT_SIZE as usize)
    }

    pub fn rent_level(nonce: u128, rent: &Rent) -> Result<u64> {
        let full = Self::full_rent(rent);
        Ok(
            ((nonce % USED_NONCES_PER_ACCOUNT as u128 + 1) * full as u128
                / USED_NONCES_PER_ACCOUNT as u128)
                .try_into()?,
        )
    }

    pub fn use_nonce<'info>(
        nonce: u128,
        loader: &AccountLoader<'info, UsedNonces>,
        config: &mut Account<'info, Config>,
        payer: AccountInfo<'info>,
        rent: &Rent,
        system_program: AccountInfo<'info>,
    ) -> Result<()> {
        if config.max_used_nonce < nonce {
            config.max_used_nonce = nonce;
        }
        let mut used_nonces = loader.load_mut()?;
        let config_rent_exempt = rent.minimum_balance(config.to_account_info().data_len());
        let current_config_lamports = config.to_account_info().lamports();
        // use max_used_nonce instead of the requested one to ignore the usage of the nonces from the gap
        let expected_config_lamports =
            config_rent_exempt + Self::rent_level(config.max_used_nonce, rent)?;
        if current_config_lamports < expected_config_lamports {
            // pay for the rent of the next account
            transfer(
                CpiContext::new(
                    system_program,
                    Transfer {
                        from: payer,
                        to: config.to_account_info(),
                    },
                ),
                expected_config_lamports - current_config_lamports,
            )?;
        } else {
            // compensate for the account creation
            let compensation = current_config_lamports - expected_config_lamports;
            if compensation > 0 {
                config.sub_lamports(compensation)?;
                payer.add_lamports(compensation)?;
            }
        }
        {
            let mut nonce_slot = unsafe {
                used_nonces
                    .used
                    .get_unchecked_mut(nonce as usize % USED_NONCES_PER_ACCOUNT)
            };
            require!(!nonce_slot.replace(true), ErrorCode::NonceAlreadyUsed);
        }

        Ok(())
    }
}
