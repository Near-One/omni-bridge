use near_sdk::{AccountId, NearToken};
use omni_utils::ErrorDisplay;
use strum_macros::AsRefStr;

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum BridgeError {
    Borsh,
    Cast,
    CannotDetermineOriginChain,
    DeployerNotSet,
    ExpectedToOverwriteTokenAddress,
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
    InvalidRecipientAddress,
    InvalidRecipientChain,
    InvalidState,
    InvalidStorageAccountsLen,
    KeyExists,
    LowerFee,
    NativeFeeForUtxoChain,
    NativeTokenRequiredForChain,
    NearWithdrawFailed,
    NotEnoughAttachedDeposit,
    OldTokenNotDeployed,
    OnlyFeeRecipientCanClaim,
    ParseAccountId,
    ParseMsg,
    ProverForChainKindNotRegistered,
    ReadPromiseRegister,
    ReadPromiseYieldId,
    RelayerAlreadyActive,
    RelayerApplicationExists,
    RelayerApplicationNotFound,
    RelayerInsufficientStake,
    RelayerNotActive,
    RelayerNotRegistered,
    SenderCanUpdateTokenFeeOnly,
    SenderIsNotConnector,
    StorageFeeRecipientOmitted,
    StorageNativeFeeRecipientOmitted,
    StoragePendingTransfers,
    StorageRecipientOmitted,
    TokenAlreadyMigrated,
    TokenDecimalsNotFound,
    TokenExists,
    TokenNotFound,
    TokenNotMigrated,
    TokenNotRegistered,
    TransferAlreadyFinalised,
    TransferNotExist,
    UnknownFactory,
    UtxoConfigMissing,
    UtxoTransferAlreadyFinalised,
    UnsupportedFeeUpdateProof,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum TokenLockError {
    InsufficientLockedTokens,
    LockedTokensOverflow,
    TokenAlreadyLocked,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum StorageError {
    AccountNotRegistered(AccountId),
    MessageAccountNotRegistered,
    NotEnoughBalanceForFee,
    NotEnoughStorageBalance {
        requested: NearToken,
        available: NearToken,
    },
    NotEnoughStorageBalanceAttached {
        required: NearToken,
        attached: NearToken,
    },
    SignerNotEnoughBalance,
    SignerNotRegistered,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum TokenError {
    FailedToReadState,
    InvalidCodeHash,
    InvalidParentAccount,
    MissingPermission,
    NoInput,
    NoStateToMigrate,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum ProverError {
    HashNotSet,
    InvalidBlockHash,
    InvalidProof,
    ParseArgs,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum TypesError {
    InvalidHex,
    InvalidHexLength,
}

#[derive(Debug, Clone, PartialEq, Eq, AsRefStr, ErrorDisplay)]
#[strum(serialize_all = "shouty_snake_case", prefix = "ERR_")]
#[non_exhaustive]
pub enum StorageBalanceError {
    AccountNotRegistered(AccountId),
    NotEnoughStorage {
        required: NearToken,
        available: NearToken,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
