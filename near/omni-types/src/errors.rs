use core::fmt;
use near_sdk::{AccountId, NearToken};

pub trait ErrorCode {
    fn code(&self) -> &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    InvalidMetadata,
    InvalidFee,
    SenderCanUpdateTokenFeeOnly,
    InvalidAttachedDeposit,
    InvalidAmountToTransfer,
    InvalidRecipientChain,
    InvalidStorageAccountsLen,
    UnknownFactory,
    UnknownFactoryErr,
    InvalidFastTransferAmount,
    IncorrectTargetAddress,
    InvalidMaxGasFee,
    InvalidProofMessage,
    InvalidProofMessageError,
    StorageRecipientOmitted,
    StorageFeeRecipientOmitted,
    StorageNativeFeeRecipientOmitted,
    OnlyBtcToBitcoin,
    NativeTokenRequiredForChain,
    OnlyFeeRecipientCanClaim,
    InsufficientStorageDeposit,
    NotEnoughAttachedDeposit,
    FastTransferAlreadyFinalised,
    SenderIsNotConnector,
    KeyExists,
    TransferAlreadyFinalised,
    UtxoTransferAlreadyFinalised,
    FastTransferPerformed,
    TokenExists,
    StoragePendingTransfers,
    FailedToGetZeroAddress,
    FailedToGetTokenAddress,
    TransferAlreadyFinalisedPanic,
    InvalidState,
    NearWithdrawFailed,
    FeeRecipientNotSetOrEmpty,
    FastTransferNotFinalised,
    InvalidProof,
    DeployerNotSet,
    ParseAccount,
    ProverForChainKindNotRegistered,
    NativeFeeForUtxoChain,
    Borsh,
    Cast,
    ParseAccountId,
    ParseMsg,
    LowerFee,
    TokenDecimalsNotFound,
    ReadPromiseRegister,
    ReadPromiseYieldId,
    TokenNotFound,
    TokenNotRegistered,
    TransferNotExist,
    FastTransferNotFound,
    UtxoConfigMissing,
}

impl ErrorCode for BridgeError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidMetadata => "ERR_INVALID_METADATA",
            Self::InvalidFee => "ERR_INVALID_FEE",
            Self::SenderCanUpdateTokenFeeOnly => "Only sender can update token fee",
            Self::InvalidAttachedDeposit => "ERR_INVALID_ATTACHED_DEPOSIT",
            Self::InvalidAmountToTransfer => "Invalid amount to transfer",
            Self::InvalidRecipientChain => "ERR_INVALID_RECIPIENT_CHAIN",
            Self::InvalidStorageAccountsLen => "Invalid len of accounts for storage deposit",
            Self::UnknownFactory => "Unknown factory",
            Self::UnknownFactoryErr => "ERR_UNKNOWN_FACTORY",
            Self::InvalidFastTransferAmount => "ERR_INVALID_FAST_TRANSFER_AMOUNT",
            Self::IncorrectTargetAddress => "Incorrect target address",
            Self::InvalidMaxGasFee => "Invalid max gas fee",
            Self::InvalidProofMessage => "Invalid proof message",
            Self::InvalidProofMessageError => "ERROR: Invalid proof message",
            Self::StorageRecipientOmitted => "STORAGE_ERR: The transfer recipient is omitted",
            Self::StorageFeeRecipientOmitted => "STORAGE_ERR: The fee recipient is omitted",
            Self::StorageNativeFeeRecipientOmitted => {
                "STORAGE_ERR: The native fee recipient is omitted"
            }
            Self::OnlyBtcToBitcoin => "Only BTC can be transferred to the Bitcoin network.",
            Self::NativeTokenRequiredForChain => {
                "Only the native token of this UTXO chain can be transferred."
            }
            Self::OnlyFeeRecipientCanClaim => "ERR_ONLY_FEE_RECIPIENT_CAN_CLAIM",
            Self::InsufficientStorageDeposit => {
                "ERROR: The deposit is not sufficient to cover the storage."
            }
            Self::NotEnoughAttachedDeposit => "ERR_NOT_ENOUGH_ATTACHED_DEPOSIT",
            Self::FastTransferAlreadyFinalised => "ERR_FAST_TRANSFER_ALREADY_FINALISED",
            Self::SenderIsNotConnector => "ERR_SENDER_IS_NOT_CONNECTOR",
            Self::KeyExists => "ERR_KEY_EXIST",
            Self::TransferAlreadyFinalised => "The transfer is already finalised",
            Self::UtxoTransferAlreadyFinalised => "The UTXO transfer is already finalised",
            Self::FastTransferPerformed => "Fast transfer is already performed",
            Self::TokenExists => "ERR_TOKEN_EXIST",
            Self::StoragePendingTransfers => {
                "This account owns some pending transfers, use `force=true` to ignore them."
            }
            Self::FailedToGetZeroAddress => "ERR_FAILED_TO_GET_ZERO_ADDRESS",
            Self::FailedToGetTokenAddress => "ERR_FAILED_TO_GET_TOKEN_ADDRESS",
            Self::TransferAlreadyFinalisedPanic => "ERR_TRANSFER_ALREADY_FINALISED",
            Self::InvalidState => "ERR_INVALID_STATE",
            Self::NearWithdrawFailed => "ERR_NEAR_WITHDRAW_FAILED",
            Self::FeeRecipientNotSetOrEmpty => "ERR_FEE_RECIPIENT_NOT_SET_OR_EMPTY",
            Self::FastTransferNotFinalised => "ERR_FAST_TRANSFER_NOT_FINALISED",
            Self::InvalidProof => "ERR_INVALID_PROOF",
            Self::DeployerNotSet => "ERR_DEPLOYER_NOT_SET",
            Self::ParseAccount => "ERR_PARSE_ACCOUNT",
            Self::ProverForChainKindNotRegistered => "ERR_PROVER_FOR_CHAIN_KIND_NOT_REGISTERED",
            Self::NativeFeeForUtxoChain => "Can't have native fee for transfers from UTXO chains",
            Self::Borsh => "ERR_BORSH",
            Self::Cast => "ERR_CAST",
            Self::ParseAccountId => "ERR_PARSE_ACCOUNT_ID",
            Self::ParseMsg => "ERR_PARSE_MSG",
            Self::LowerFee => "ERR_LOWER_FEE",
            Self::TokenDecimalsNotFound => "ERR_TOKEN_DECIMALS_NOT_FOUND",
            Self::ReadPromiseRegister => "ERR_READ_PROMISE_REGISTER",
            Self::ReadPromiseYieldId => "ERR_READ_PROMISE_YIELD_ID",
            Self::TokenNotFound => "ERR_TOKEN_NOT_FOUND",
            Self::TokenNotRegistered => "ERR_TOKEN_NOT_REGISTERED",
            Self::TransferNotExist => "ERR_TRANSFER_NOT_EXIST",
            Self::FastTransferNotFound => "ERR_FAST_TRANSFER_NOT_FOUND",
            Self::UtxoConfigMissing => "ERR_UTXO_CONFIG_MISSING",
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    MessageAccountNotRegistered,
    NotEnoughBalanceForFee,
    SignerNotRegistered,
    SignerNotEnoughBalance,
}

impl ErrorCode for StorageError {
    fn code(&self) -> &'static str {
        match self {
            Self::MessageAccountNotRegistered => "ERR_MESSAGE_ACCOUNT_NOT_REGISTERED",
            Self::NotEnoughBalanceForFee => "ERR_NOT_ENOUGH_BALANCE_FOR_FEE",
            Self::SignerNotRegistered => "ERR_SIGNER_NOT_REGISTERED",
            Self::SignerNotEnoughBalance => "ERR_SIGNER_NOT_ENOUGH_BALANCE",
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    MissingPermission,
    InvalidParentAccount,
    NoInput,
    FailedToReadState,
}

impl ErrorCode for TokenError {
    fn code(&self) -> &'static str {
        match self {
            Self::MissingPermission => "ERR_MISSING_PERMISSION",
            Self::InvalidParentAccount => "ERR_INVALID_PARENT_ACCOUNT",
            Self::NoInput => "ERR_NO_INPUT",
            Self::FailedToReadState => "ERR_FAILED_TO_READ_STATE",
        }
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverError {
    ParseArgs,
    InvalidProof,
    HashNotSet,
    InvalidBlockHash,
}

impl ErrorCode for ProverError {
    fn code(&self) -> &'static str {
        match self {
            Self::ParseArgs => "ERR_PARSE_ARGS",
            Self::InvalidProof => "ERR_INVALID_PROOF",
            Self::HashNotSet => "ERR_HASH_NOT_SET",
            Self::InvalidBlockHash => "ERR_INVALID_BLOCK_HASH",
        }
    }
}

impl fmt::Display for ProverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypesError {
    InvalidHex,
    InvalidHexLength,
}

impl ErrorCode for TypesError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidHex => "ERR_INVALIDE_HEX",
            Self::InvalidHexLength => "ERR_INVALID_HEX_LENGTH",
        }
    }
}

impl fmt::Display for TypesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmniError {
    Bridge(BridgeError),
    Storage(StorageError),
    Token(TokenError),
    Prover(ProverError),
    Types(TypesError),
    StorageBalance(StorageBalanceError),
}

impl ErrorCode for OmniError {
    fn code(&self) -> &'static str {
        match self {
            Self::Bridge(err) => err.code(),
            Self::Storage(err) => err.code(),
            Self::Token(err) => err.code(),
            Self::Prover(err) => err.code(),
            Self::Types(err) => err.code(),
            Self::StorageBalance(err) => err.code(),
        }
    }
}

impl fmt::Display for OmniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl From<BridgeError> for OmniError {
    fn from(value: BridgeError) -> Self {
        Self::Bridge(value)
    }
}

impl From<StorageError> for OmniError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<TokenError> for OmniError {
    fn from(value: TokenError) -> Self {
        Self::Token(value)
    }
}

impl From<ProverError> for OmniError {
    fn from(value: ProverError) -> Self {
        Self::Prover(value)
    }
}

impl From<TypesError> for OmniError {
    fn from(value: TypesError) -> Self {
        Self::Types(value)
    }
}

impl From<StorageBalanceError> for OmniError {
    fn from(value: StorageBalanceError) -> Self {
        Self::StorageBalance(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageBalanceError {
    AccountNotRegistered(AccountId),
    NotEnoughStorage {
        required: NearToken,
        available: NearToken,
    },
}

impl ErrorCode for StorageBalanceError {
    fn code(&self) -> &'static str {
        match self {
            Self::AccountNotRegistered(_) => "ERR_ACCOUNT_NOT_REGISTERED",
            Self::NotEnoughStorage { .. } => "ERR_NOT_ENOUGH_STORAGE",
        }
    }
}

impl fmt::Display for StorageBalanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountNotRegistered(account_id) => {
                write!(f, "Account {} is not registered", account_id)
            }
            Self::NotEnoughStorage {
                required,
                available,
            } => write!(
                f,
                "Not enough storage deposited, required: {}, available: {}",
                required, available
            ),
        }
    }
}
