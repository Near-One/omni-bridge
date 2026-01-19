use near_sdk::{AccountId, NearToken};

use crate::OmniAddress;

macro_rules! define_contract_errors {
    ($prefix:expr, $enum_name:ident {
        $(
            $variant:ident $( { $($arg:ident : $typ:ty),* $(,)? } )?
        ),* $(,)?
    }) => {
        #[derive(near_sdk::serde::Serialize, Debug, Clone, PartialEq, Eq)]
        #[serde(crate = "near_sdk::serde", rename_all = "SCREAMING_SNAKE_CASE")]
        #[non_exhaustive]
        pub enum $enum_name {
            $(
                $variant $( { $($arg: $typ),* } )?
            ),*
        }

        impl std::fmt::Display for $enum_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        Self::$variant $( { $($arg),* } )? => {
                            let mut shouty = String::new();
                            for (i, c) in stringify!($variant).chars().enumerate() {
                                if i > 0 && c.is_uppercase() {
                                    shouty.push('_');
                                }
                                shouty.push(c.to_ascii_uppercase());
                            }

                            write!(f, "{}_{}", $prefix, shouty)?;

                            $(
                                let mut first = true;
                                $(
                                    if first {
                                        write!(f, ": ")?;
                                        first = false;
                                    } else {
                                        write!(f, ", ")?;
                                    }
                                    write!(f, "{}={}", stringify!($arg), $arg)?;
                                )*
                                let _ = first;
                            )?

                            Ok(())
                        }
                    ),*
                }
            }
        }
    };
}

define_contract_errors! {
    "ERR",
    BridgeError {
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
        OnlyFeeRecipientCanClaim,
        ParseAccountId,
        ParseMsg,
        ProverForChainKindNotRegistered,
        ReadPromiseRegister,
        ReadPromiseYieldId,
        SenderCanUpdateTokenFeeOnly,
        SenderIsNotConnector,
        StorageNativeFeeRecipientOmitted,
        StoragePendingTransfers,
        StorageOmitted { address: OmniAddress },
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
}

define_contract_errors! {
    "ERR",
    StorageError {
        MessageAccountNotRegistered,
        NotEnoughBalanceForFee,
        SignerNotEnoughBalance,
        SignerNotRegistered,
    }
}

define_contract_errors! {
    "ERR",
    TokenError {
        FailedToReadState,
        InvalidCodeHash,
        InvalidParentAccount,
        MissingPermission,
        NoInput,
        NoStateToMigrate,
    }
}

define_contract_errors! {
    "ERR",
    ProverError {
        HashNotSet,
        InvalidBlockHash,
        InvalidProof,
        ParseArgs,
    }
}

define_contract_errors! {
    "ERR",
    TypesError {
        InvalidHex,
        InvalidHexLength,
    }
}

define_contract_errors! {
    "ERR",
    StorageBalanceError {
        AccountNotRegistered {
            account_id: AccountId,
        },
        NotEnoughStorage {
            required: NearToken,
            available: NearToken,
        },
    }
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
