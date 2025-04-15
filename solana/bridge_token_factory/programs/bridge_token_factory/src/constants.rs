use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const SOL_VAULT_SEED: &[u8] = b"sol_vault";

#[constant]
pub const USED_NONCES_SEED: &[u8] = b"used_nonces";

#[constant]
pub const WRAPPED_MINT_SEED: &[u8] = b"wrapped_mint";

#[constant]
pub const METADATA_SEED: &[u8] = b"metadata";

#[constant]
pub const USED_NONCES_PER_ACCOUNT: u32 = 1024;

#[constant]
pub const USED_NONCES_ACCOUNT_SIZE: u32 = 8 + (USED_NONCES_PER_ACCOUNT + 7).div_ceil(8);

#[constant]
pub const SOLANA_OMNI_BRIDGE_CHAIN_ID: u8 = 2;

#[constant]
pub const MAX_ALLOWED_DECIMALS: u8 = 9;

#[constant]
pub const INIT_TRANSFER_PAUSED: u8 = 1 << 0;

#[constant]
pub const FINALIZE_TRANSFER_PAUSED: u8 = 1 << 1;

#[constant]
pub const ALL_PAUSED: u8 = INIT_TRANSFER_PAUSED | FINALIZE_TRANSFER_PAUSED;

#[constant]
pub const ALL_UNPAUSED: u8 = 0;
