use near_sdk::{AccountId, NearToken};
use strum_macros::{AsRefStr, Display};

#[derive(Display, Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum BridgeError {
    Borsh,
    Cast,
    DeployerNotSet,
    FailedToGetTokenAddress,
    FailedToGetZeroAddress,
    FastTransferAlreadyFinalised,
    FastTransferAlreadyPerformed,
    FastTransferNotFinalised,
    FastTransferNotFound,
    FeeRecipientNotSetOrEmpty,
    IncorrectTargetUtxoAddress,
    InsufficientStorageDeposit,
    InvalidAmountToTransfer,
    InvalidAttachedDeposit,
    InvalidFastTransferAmount,
    InvalidFee,
    InvalidMaxGasFee,
    InvalidMetadata,
    InvalidProof,
    InvalidProofMessage,
    InvalidRecipientChain,
    InvalidState,
    InvalidStorageAccountsLen,
    KeyExists,
    LowerFee,
    NativeFeeForUtxoChain,
    NativeTokenRequiredForChain,
    NearWithdrawFailed,
    NotEnoughAttachedDeposit,
    OnlyBtcToBitcoin,
    OnlyFeeRecipientCanClaim,
    ParseAccountId,
    ParseMsg,
    ProverForChainKindNotRegistered,
    ReadPromiseRegister,
    ReadPromiseYieldId,
    SenderCanUpdateTokenFeeOnly,
    SenderIsNotConnector,
    StorageFeeRecipientOmitted,
    StorageNativeFeeRecipientOmitted,
    StoragePendingTransfers,
    StorageRecipientOmitted,
    TokenDecimalsNotFound,
    TokenExists,
    TokenNotFound,
    TokenNotRegistered,
    TransferAlreadyFinalised,
    TransferNotExist,
    UnknownFactory,
    UtxoConfigMissing,
    UtxoTransferAlreadyFinalised,
}

#[derive(Display, Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum StorageError {
    MessageAccountNotRegistered,
    NotEnoughBalanceForFee,
    SignerNotEnoughBalance,
    SignerNotRegistered,
}

#[derive(Display, Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum TokenError {
    FailedToReadState,
    InvalidParentAccount,
    MissingPermission,
    NoInput,
}

#[derive(Display, Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum ProverError {
    HashNotSet,
    InvalidBlockHash,
    InvalidProof,
    ParseArgs,
}

#[derive(Display, Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum TypesError {
    InvalidHex,
    InvalidHexLength,
}

#[derive(Display, Debug, Clone, PartialEq, Eq, AsRefStr)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
pub enum StorageBalanceError {
    AccountNotRegistered(AccountId),
    NotEnoughStorage {
        required: NearToken,
        available: NearToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmniError {
    Bridge(BridgeError),
    Prover(ProverError),
    Storage(StorageError),
    StorageBalance(StorageBalanceError),
    Token(TokenError),
    Types(TypesError),
}

impl std::fmt::Display for OmniError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OmniError::Bridge(err) => write!(f, "{err}"),
            OmniError::Prover(err) => write!(f, "{err}"),
            OmniError::Storage(err) => write!(f, "{err}"),
            OmniError::StorageBalance(err) => write!(f, "{err}"),
            OmniError::Token(err) => write!(f, "{err}"),
            OmniError::Types(err) => write!(f, "{err}"),
        }
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
