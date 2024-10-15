use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    NonceAlreadyUsed,
    Unauthorized,
    TokenMetadataNotProvided,
}