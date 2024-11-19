use anchor_lang::prelude::*;

#[error_code(offset = 6000)]
pub enum ErrorCode {
    #[msg("Invalid arguments")]
    InvalidArgs,
    #[msg("Signature verification failed")]
    SignatureVerificationFailed,
    #[msg("Nonce already used")]
    NonceAlreadyUsed,
    #[msg("Token metadata not provided")]
    TokenMetadataNotProvided,
    #[msg("Invalid token metadata address")]
    InvalidTokenMetadataAddress,
    #[msg("Invalid bridged token")]
    InvalidBridgedToken,
    #[msg("Invalid fee")]
    InvalidFee,
}
