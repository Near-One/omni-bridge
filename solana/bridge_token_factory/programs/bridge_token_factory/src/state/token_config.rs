use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct TokenConfig {
    pub origin_decimals: u8,
    pub dust: u128,
}
