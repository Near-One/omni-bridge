use anchor_lang::prelude::*;

#[constant]
pub const DERIVED_NEAR_BRIDGE_ADDRESS: [u8; 64] = [
    251, 68, 120, 58, 81, 118, 152, 127, 82, 144, 201, 3, 155, 120, 205, 68, 127, 0, 13, 46, 181,
    138, 131, 83, 41, 60, 134, 18, 214, 185, 83, 102, 221, 254, 189, 217, 72, 147, 49, 87, 118,
    107, 41, 226, 91, 100, 139, 234, 44, 140, 74, 101, 135, 211, 213, 40, 231, 252, 77, 11, 96,
    209, 90, 183,
];

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
pub const USED_NONCES_PER_ACCOUNT: u32 = 1024;

#[constant]
pub const USED_NONCES_ACCOUNT_SIZE: u32 = 8 + (USED_NONCES_PER_ACCOUNT + 7) / 8;

#[constant]
pub const DEFAULT_ADMIN: Pubkey = pubkey!("2ajXVaqXXpHWtPnW3tKZukuXHGGjVcENjuZaWrz6NhD4"); // TODO update this to the pubkey you can sign with