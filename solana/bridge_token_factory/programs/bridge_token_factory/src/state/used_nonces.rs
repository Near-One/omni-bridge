use anchor_lang::prelude::*;
use anchor_lang::system_program::transfer;
use anchor_lang::system_program::Transfer;
#[cfg(not(feature = "idl-build"))]
use bitvec::array::BitArray;

use crate::constants::AUTHORITY_SEED;
use crate::constants::{USED_NONCES_ACCOUNT_SIZE, USED_NONCES_PER_ACCOUNT};
#[cfg(not(feature = "idl-build"))]
use crate::error::ErrorCode;

use super::config::Config;

#[cfg(not(feature = "idl-build"))]
#[allow(clippy::as_conversions)]
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct UsedNonces {
    used: BitArray<[u8; (USED_NONCES_PER_ACCOUNT as usize + 7).div_ceil(8)]>,
}

#[cfg(feature = "idl-build")]
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct UsedNonces {}

impl UsedNonces {
    #[allow(clippy::as_conversions)]
    pub fn full_rent(rent: &Rent) -> u64 {
        rent.minimum_balance(USED_NONCES_ACCOUNT_SIZE as usize)
    }

    pub fn rent_level(nonce: u64, rent: &Rent) -> Result<u64> {
        let full = Self::full_rent(rent);
        Ok((nonce % u64::from(USED_NONCES_PER_ACCOUNT) + 1) * full
            / u64::from(USED_NONCES_PER_ACCOUNT))
    }

    pub fn use_nonce<'info>(
        nonce: u64,
        loader: &AccountLoader<'info, UsedNonces>,
        config: &mut Account<'info, Config>,
        rent_reserve: AccountInfo<'info>,
        payer: AccountInfo<'info>,
        rent: &Rent,
        system_program: AccountInfo<'info>,
    ) -> Result<()> {
        if config.max_used_nonce < nonce {
            config.max_used_nonce = nonce;
        }
        // use max_used_nonce instead of the requested one to ignore the usage of the nonces from the gap
        let expected_rent_reserve_lamports =
            rent.minimum_balance(0) + Self::rent_level(config.max_used_nonce, rent)?;
        let current_rent_reserve_lamports = rent_reserve.lamports();
        if current_rent_reserve_lamports < expected_rent_reserve_lamports {
            // pay for the rent of the next account
            transfer(
                CpiContext::new(
                    system_program,
                    Transfer {
                        from: payer,
                        to: rent_reserve,
                    },
                ),
                expected_rent_reserve_lamports - current_rent_reserve_lamports,
            )?;
        } else {
            // compensate for the account creation
            let compensation = current_rent_reserve_lamports - expected_rent_reserve_lamports;
            if compensation > 0 {
                // compensate expenses for the account creation
                transfer(
                    CpiContext::new_with_signer(
                        system_program,
                        Transfer {
                            from: rent_reserve,
                            to: payer,
                        },
                        &[&[AUTHORITY_SEED, &[config.bumps.authority]]],
                    ),
                    compensation,
                )?;
            }
        }
        #[cfg(not(feature = "idl-build"))]
        {
            let mut used_nonces = match loader.load_init() {
                Ok(used_nonces) => used_nonces,
                Err(Error::AnchorError(e))
                    if e.error_code_number
                        == u32::from(
                            anchor_lang::error::ErrorCode::AccountDiscriminatorAlreadySet,
                        ) =>
                {
                    loader.load_mut()?
                }
                Err(e) => return Err(e.with_account_name("used_nonces")),
            };
            let mut nonce_slot = unsafe {
                used_nonces
                    .used
                    .get_unchecked_mut(usize::try_from(nonce % u64::from(USED_NONCES_PER_ACCOUNT))?)
            };
            require!(!nonce_slot.replace(true), ErrorCode::NonceAlreadyUsed);
        }
        #[cfg(feature = "idl-build")]
        {
            loader.load_mut()?; // fix unused variable warning
        }

        Ok(())
    }
}
