use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const MESSAGE_SEED: &[u8] = b"message";

#[constant]
pub const USED_NONCES_SEED: &[u8] = b"used_nonces";


#[constant]
pub const USED_NONCES_PER_ACCOUNT: usize = 1024;

#[constant]
pub const USED_NONCES_ACCOUNT_SIZE: usize = 8 + (USED_NONCES_PER_ACCOUNT + 7) / 8;

#[constant]
pub const DEFAULT_ADMIN: Pubkey = pubkey!("2ajXVaqXXpHWtPnW3tKZukuXHGGjVcENjuZaWrz6NhD4"); // TODO update this to the pubkey you can sign with