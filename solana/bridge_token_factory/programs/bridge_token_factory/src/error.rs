use anchor_lang::prelude::*;

#[error_code(offset = 6000)]
pub enum ErrorCode {
    #[msg("Invalid arguments")]
    InvalidArgs,
    #[msg("Signature verification failed")]
    SignatureVerificationFailed,
    NonceAlreadyUsed,
    Unauthorized,
    TokenMetadataNotProvided,
    SolanaTokenParsingFailed,
    BridgedTokenHasVault,
    NativeTokenHasNoVault,
    InvalidBridgedToken,
}
