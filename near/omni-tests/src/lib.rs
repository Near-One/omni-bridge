// Allow unused_must_use in tests - transaction results are often not checked
// because tests use assertions to verify behavior instead
#![cfg_attr(test, allow(unused_must_use))]

#[cfg(test)]
mod environment;
mod fast_transfer;
mod fin_transfer;
mod helpers;
mod init_transfer;
mod native_fee_role;
mod omni_token;
mod utxo_fin_transfer;
