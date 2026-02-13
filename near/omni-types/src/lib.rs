use std::string::ToString;

use borsh::{BorshDeserialize, BorshSerialize};
use core::fmt;
use core::str::FromStr;

use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_with::hex::Hex;
use near_sdk::{near, serde_json, AccountId};
use num_enum::IntoPrimitive;
use schemars::JsonSchema;
use sol_address::SolAddress;

pub mod btc;
pub mod errors;
pub mod evm;
pub mod hex_types;
pub mod locker_args;
pub mod mpc_types;
pub mod near_events;
pub mod prover_args;
pub mod prover_result;
pub mod sol_address;
pub mod utils;

#[cfg(test)]
mod tests;

pub use errors::{
    BridgeError, OmniError, ProverError, StorageBalanceError, StorageError, TokenError, TypesError,
};
pub use hex_types::{H160, H256};

#[near(serializers = [borsh, json])]
#[derive(
    Debug,
    Eq,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Ord,
    strum_macros::AsRefStr,
    Default,
    IntoPrimitive,
    Hash,
)]
#[repr(u8)]
pub enum ChainKind {
    #[default]
    #[serde(alias = "eth")]
    Eth,
    #[serde(alias = "near")]
    Near,
    #[serde(alias = "sol")]
    Sol,
    #[serde(alias = "arb")]
    Arb,
    #[serde(alias = "base")]
    Base,
    #[serde(alias = "bnb")]
    Bnb,
    #[serde(alias = "btc")]
    Btc,
    #[serde(alias = "zcash")]
    Zcash,
    #[serde(alias = "pol")]
    Pol,
    #[serde(rename = "HlEvm")]
    #[serde(alias = "hlevm")]
    #[strum(serialize = "HlEvm")]
    HyperEvm,
    #[serde(alias = "strk")]
    Strk,
}

impl ChainKind {
    pub const fn is_evm_chain(&self) -> bool {
        match self {
            Self::Eth | Self::Arb | Self::Base | Self::Bnb | Self::Pol | Self::HyperEvm => true,
            Self::Btc | Self::Zcash | Self::Near | Self::Sol | Self::Strk => false,
        }
    }

    pub const fn is_utxo_chain(&self) -> bool {
        match self {
            Self::Btc | Self::Zcash => true,
            Self::Eth
            | Self::Arb
            | Self::Base
            | Self::Bnb
            | Self::Pol
            | Self::Near
            | Self::Sol
            | Self::HyperEvm
            | Self::Strk => false,
        }
    }
}

impl FromStr for ChainKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        near_sdk::serde_json::from_str(&format!("\"{s}\"")).map_err(stringify)
    }
}

impl From<&OmniAddress> for ChainKind {
    fn from(input: &OmniAddress) -> Self {
        input.get_chain()
    }
}

impl TryFrom<u8> for ChainKind {
    type Error = String;
    fn try_from(input: u8) -> Result<Self, String> {
        match input {
            0 => Ok(Self::Eth),
            1 => Ok(Self::Near),
            2 => Ok(Self::Sol),
            3 => Ok(Self::Arb),
            4 => Ok(Self::Base),
            5 => Ok(Self::Bnb),
            6 => Ok(Self::Btc),
            7 => Ok(Self::Zcash),
            8 => Ok(Self::Pol),
            9 => Ok(Self::HyperEvm),
            10 => Ok(Self::Strk),
            _ => Err(format!("{input:?} invalid chain kind")),
        }
    }
}

pub type EvmAddress = H160;
pub type UTXOChainAddress = String;
pub type StarknetAddress = H256;

pub const ZERO_ACCOUNT_ID: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[near(serializers=[borsh])]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum OmniAddress {
    Eth(EvmAddress),
    Near(AccountId),
    Sol(SolAddress),
    Arb(EvmAddress),
    Base(EvmAddress),
    Bnb(EvmAddress),
    Btc(UTXOChainAddress),
    Zcash(UTXOChainAddress),
    Pol(EvmAddress),
    HyperEvm(EvmAddress),
    Strk(StarknetAddress),
}

impl OmniAddress {
    #[allow(clippy::missing_panics_doc)]
    pub fn new_zero(chain_kind: ChainKind) -> Result<Self, String> {
        match chain_kind {
            ChainKind::Eth => Ok(Self::Eth(H160::ZERO)),
            ChainKind::Near => Ok(Self::Near(ZERO_ACCOUNT_ID.parse().map_err(stringify)?)),
            ChainKind::Sol => Ok(Self::Sol(SolAddress::ZERO)),
            ChainKind::Arb => Ok(Self::Arb(H160::ZERO)),
            ChainKind::Base => Ok(Self::Base(H160::ZERO)),
            ChainKind::Bnb => Ok(Self::Bnb(H160::ZERO)),
            ChainKind::Pol => Ok(Self::Pol(H160::ZERO)),
            ChainKind::HyperEvm => Ok(Self::HyperEvm(H160::ZERO)),
            ChainKind::Btc => Ok(Self::Btc(String::new())),
            ChainKind::Zcash => Ok(Self::Zcash(String::new())),
            ChainKind::Strk => Ok(Self::Strk(H256::ZERO)),
        }
    }

    pub fn new_from_evm_address(
        chain_kind: ChainKind,
        address: EvmAddress,
    ) -> Result<Self, String> {
        match chain_kind {
            ChainKind::Eth => Ok(Self::Eth(address)),
            ChainKind::Arb => Ok(Self::Arb(address)),
            ChainKind::Base => Ok(Self::Base(address)),
            ChainKind::Bnb => Ok(Self::Bnb(address)),
            ChainKind::Pol => Ok(Self::Pol(address)),
            ChainKind::HyperEvm => Ok(Self::HyperEvm(address)),
            _ => Err(format!("{chain_kind:?} is not an EVM chain")),
        }
    }

    pub fn new_from_slice(chain_kind: ChainKind, address: &[u8]) -> Result<Self, String> {
        match chain_kind {
            ChainKind::Sol => Ok(Self::Sol(Self::to_sol_address(address)?)),
            ChainKind::Eth
            | ChainKind::Arb
            | ChainKind::Base
            | ChainKind::Bnb
            | ChainKind::Pol
            | ChainKind::HyperEvm => {
                Self::new_from_evm_address(chain_kind, Self::to_evm_address(address)?)
            }
            ChainKind::Near => Ok(Self::Near(Self::to_near_account_id(address)?)),
            ChainKind::Btc => Ok(Self::Btc(
                String::from_utf8(address.to_vec())
                    .map_err(|e| format!("Invalid BTC address: {e}"))?,
            )),
            ChainKind::Zcash => Ok(Self::Zcash(
                String::from_utf8(address.to_vec())
                    .map_err(|e| format!("Invalid ZCash address: {e}"))?,
            )),
            ChainKind::Strk => Ok(Self::Strk(H256(address.try_into().map_err(stringify)?))),
        }
    }

    pub const fn get_chain(&self) -> ChainKind {
        match self {
            Self::Eth(_) => ChainKind::Eth,
            Self::Near(_) => ChainKind::Near,
            Self::Sol(_) => ChainKind::Sol,
            Self::Arb(_) => ChainKind::Arb,
            Self::Base(_) => ChainKind::Base,
            Self::Bnb(_) => ChainKind::Bnb,
            Self::Pol(_) => ChainKind::Pol,
            Self::HyperEvm(_) => ChainKind::HyperEvm,
            Self::Btc(_) => ChainKind::Btc,
            Self::Zcash(_) => ChainKind::Zcash,
            Self::Strk(_) => ChainKind::Strk,
        }
    }

    pub fn encode(&self, separator: char, skip_zero_address: bool) -> String {
        let (chain_str, address) = match self {
            Self::Eth(address) => ("eth", address.to_string()),
            Self::Near(address) => ("near", address.to_string()),
            Self::Sol(address) => ("sol", address.to_string()),
            Self::Arb(address) => ("arb", address.to_string()),
            Self::Base(address) => ("base", address.to_string()),
            Self::Bnb(address) => ("bnb", address.to_string()),
            Self::Pol(address) => ("pol", address.to_string()),
            Self::HyperEvm(address) => ("hlevm", address.to_string()),
            Self::Btc(address) => ("btc", address.to_string()),
            Self::Zcash(address) => ("zcash", address.to_string()),
            Self::Strk(address) => ("strk", address.to_string()),
        };

        if skip_zero_address && self.is_zero() {
            chain_str.to_string()
        } else {
            format!("{chain_str}{separator}{address}")
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Self::Eth(address)
            | Self::Arb(address)
            | Self::Base(address)
            | Self::Bnb(address)
            | Self::Pol(address)
            | Self::HyperEvm(address) => address.is_zero(),
            Self::Near(address) => *address == ZERO_ACCOUNT_ID,
            Self::Sol(address) => address.is_zero(),
            Self::Btc(address) | Self::Zcash(address) => address.is_empty(),
            Self::Strk(address) => address.is_zero(),
        }
    }

    pub fn get_token_prefix(&self) -> String {
        match self {
            Self::Sol(address) => Self::hashed_token_prefix("sol", &H256(address.0)),
            Self::Strk(address) => Self::hashed_token_prefix("strk", address),
            Self::Eth(address) => {
                if self.is_zero() {
                    "eth".to_string()
                } else {
                    address.to_string()[2..].to_string()
                }
            }
            _ => self.encode('-', true),
        }
    }

    pub fn get_utxo_address(&self) -> Option<UTXOChainAddress> {
        match self {
            Self::Btc(btc_address) => Some(btc_address.clone()),
            Self::Zcash(zcash_address) => Some(zcash_address.clone()),
            _ => None,
        }
    }

    pub fn is_evm_chain(&self) -> bool {
        self.get_chain().is_evm_chain()
    }

    pub fn is_utxo_chain(&self) -> bool {
        self.get_chain().is_utxo_chain()
    }

    // The AccountId on Near can't be uppercased and has a 64 character limit,
    // so we encode the address into 20 bytes to bypass these restrictions
    fn hashed_token_prefix(prefix: &str, address: &H256) -> String {
        if address.is_zero() {
            prefix.to_string()
        } else {
            let hashed_address = H160(
                utils::keccak256(&address.0)[12..]
                    .try_into()
                    .unwrap_or_default(),
            )
            .to_string();
            format!("{prefix}-{hashed_address}")
        }
    }

    fn to_evm_address(address: &[u8]) -> Result<EvmAddress, String> {
        let address = if address.len() == 32 {
            &address[address.len() - 20..]
        } else {
            address
        };

        address.try_into().map_or_else(
            |_| Err("Invalid EVM address".to_string()),
            |bytes| Ok(H160(bytes)),
        )
    }

    fn to_sol_address(address: &[u8]) -> Result<SolAddress, String> {
        address.try_into().map_or_else(
            |_| Err("Invalid SOL address".to_string()),
            |bytes| Ok(SolAddress(bytes)),
        )
    }

    fn to_near_account_id(address: &[u8]) -> Result<AccountId, String> {
        AccountId::from_str(&String::from_utf8(address.to_vec()).map_err(stringify)?)
            .map_err(stringify)
    }
}

impl FromStr for OmniAddress {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (chain, recipient) = input.split_once(':').unwrap_or(("eth", input));

        match chain {
            "eth" => Ok(Self::Eth(recipient.parse().map_err(stringify)?)),
            "near" => Ok(Self::Near(recipient.parse().map_err(stringify)?)),
            "sol" => Ok(Self::Sol(recipient.parse().map_err(stringify)?)),
            "arb" => Ok(Self::Arb(recipient.parse().map_err(stringify)?)),
            "base" => Ok(Self::Base(recipient.parse().map_err(stringify)?)),
            "bnb" => Ok(Self::Bnb(recipient.parse().map_err(stringify)?)),
            "pol" => Ok(Self::Pol(recipient.parse().map_err(stringify)?)),
            "hlevm" => Ok(Self::HyperEvm(recipient.parse().map_err(stringify)?)),
            "btc" => Ok(Self::Btc(recipient.to_string())),
            "zcash" => Ok(Self::Zcash(recipient.to_string())),
            "strk" => Ok(Self::Strk(recipient.parse().map_err(stringify)?)),
            _ => Err(format!("Chain {chain} is not supported")),
        }
    }
}

impl fmt::Display for OmniAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.encode(':', false))
    }
}

impl JsonSchema for OmniAddress {
    fn is_referenceable() -> bool {
        false
    }

    fn schema_name() -> String {
        String::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }
}

impl Serialize for OmniAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for OmniAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OmniAddressVisitor;

        impl serde::de::Visitor<'_> for OmniAddressVisitor {
            type Value = OmniAddress;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in the format 'chain:address'")
            }

            fn visit_str<E>(self, input: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                OmniAddress::from_str(input).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(OmniAddressVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BridgeOnTransferMsg {
    InitTransfer(InitTransferMsg),
    FastFinTransfer(FastFinTransferMsg),
    UtxoFinTransfer(UtxoFinTransferMsg),
    SwapMigratedToken,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitTransferMsg {
    pub recipient: OmniAddress,
    pub fee: U128,
    pub native_token_fee: U128,
    pub msg: Option<String>,
}

#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct FastFinTransferMsg {
    pub transfer_id: UnifiedTransferId,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub msg: String,
    pub amount: U128,
    pub storage_deposit_amount: Option<U128>,
    pub relayer: AccountId,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct UtxoFinTransferMsg {
    pub utxo_id: UtxoId,
    pub recipient: OmniAddress,
    pub relayer_fee: U128,
    pub msg: String,
}

impl UtxoFinTransferMsg {
    pub fn get_transfer_id(&self, origin_chain: ChainKind) -> UnifiedTransferId {
        UnifiedTransferId {
            origin_chain,
            kind: TransferIdKind::Utxo(self.utxo_id.clone()),
        }
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fee {
    pub fee: U128,
    pub native_fee: U128,
}

impl Fee {
    pub const fn is_zero(&self) -> bool {
        self.fee.0 == 0 && self.native_fee.0 == 0
    }
}

#[near(serializers = [borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, Default, Copy)]
pub struct TransferId {
    // The origin chain kind
    pub origin_chain: ChainKind,
    // The transfer nonce that maintained on the source chain
    pub origin_nonce: Nonce,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessage {
    pub origin_nonce: Nonce,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
    pub destination_nonce: Nonce,
    pub origin_transfer_id: Option<UnifiedTransferId>,
}

impl TransferMessage {
    pub const fn get_origin_chain(&self) -> ChainKind {
        self.sender.get_chain()
    }

    pub const fn get_transfer_id(&self) -> TransferId {
        TransferId {
            origin_chain: self.get_origin_chain(),
            origin_nonce: self.origin_nonce,
        }
    }

    pub const fn get_destination_chain(&self) -> ChainKind {
        self.recipient.get_chain()
    }

    pub fn calculate_storage_account_id(&self) -> AccountId {
        TransferMessageStorageAccount::from(self.clone()).id()
    }
}

// Used to calculate virtual account ID that can be used to deposit storage required for the message
#[near(serializers=[borsh])]
#[derive(Debug, Clone)]
pub struct TransferMessageStorageAccount {
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
}

impl TransferMessageStorageAccount {
    #[allow(clippy::missing_panics_doc)]
    pub fn id(&self) -> AccountId {
        let hash = utils::sha256(&borsh::to_vec(self).unwrap());
        let implicit_account_id = hex::encode(hash);
        AccountId::try_from(implicit_account_id).unwrap()
    }
}

impl From<TransferMessage> for TransferMessageStorageAccount {
    fn from(value: TransferMessage) -> Self {
        Self {
            token: value.token,
            amount: value.amount,
            recipient: value.recipient,
            fee: value.fee,
            sender: value.sender,
            msg: value.msg,
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Debug, Clone)]
pub enum PayloadType {
    TransferMessage,
    Metadata,
    ClaimNativeFee,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessagePayloadV1 {
    pub prefix: PayloadType,
    pub destination_nonce: Nonce,
    pub transfer_id: TransferId,
    pub token_address: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee_recipient: Option<AccountId>,
}

impl From<TransferMessagePayload> for TransferMessagePayloadV1 {
    fn from(payload: TransferMessagePayload) -> Self {
        Self {
            prefix: payload.prefix,
            destination_nonce: payload.destination_nonce,
            transfer_id: payload.transfer_id,
            token_address: payload.token_address,
            amount: payload.amount,
            recipient: payload.recipient,
            fee_recipient: payload.fee_recipient,
        }
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct TransferMessagePayload {
    pub prefix: PayloadType,
    pub destination_nonce: Nonce,
    pub transfer_id: TransferId,
    pub token_address: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee_recipient: Option<AccountId>,
    #[serde(default)]
    pub message: Vec<u8>,
}

impl TransferMessagePayload {
    pub fn encode_hashable(&self) -> Result<Vec<u8>, String> {
        if self.message.is_empty() {
            borsh::to_vec(&TransferMessagePayloadV1::from(self.clone())).map_err(stringify)
        } else {
            borsh::to_vec(self).map_err(stringify)
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Debug, Clone)]
pub struct MetadataPayload {
    pub prefix: PayloadType,
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[near(serializers=[borsh, json])]
#[derive(Clone)]
pub struct SignRequest {
    pub payload: [u8; 32],
    pub path: String,
    pub key_version: u32,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub enum UpdateFee {
    Fee(Fee),
    Proof(Vec<u8>),
}

pub type Nonce = u64;

pub fn stringify<T: std::fmt::Display>(item: T) -> String {
    item.to_string()
}

#[near(serializers=[json])]
#[derive(Clone, Debug)]
pub struct BasicMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct UtxoId {
    pub tx_hash: String,
    pub vout: u32,
}

impl std::str::FromStr for UtxoId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('@').collect();

        if parts.len() != 2 {
            return Err(format!(
                "Invalid UtxoId format '{s}': expected 'tx_hash@vout'",
            ));
        }

        let tx_hash = parts[0].to_string();
        let vout = parts[1]
            .parse::<u32>()
            .map_err(|e| format!("Invalid vout '{}' in UtxoId: {}", parts[1], e))?;

        Ok(UtxoId { tx_hash, vout })
    }
}

impl std::fmt::Display for UtxoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.tx_hash, self.vout)
    }
}

impl TryFrom<String> for UtxoId {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<UtxoId> for String {
    fn from(utxo_id: UtxoId) -> Self {
        format!("{}@{}", utxo_id.tx_hash, utxo_id.vout)
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransferIdKind {
    Nonce(Nonce),
    Utxo(UtxoId),
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnifiedTransferId {
    pub origin_chain: ChainKind,
    pub kind: TransferIdKind,
}

impl From<TransferId> for UnifiedTransferId {
    fn from(value: TransferId) -> Self {
        Self {
            origin_chain: value.origin_chain,
            kind: TransferIdKind::Nonce(value.origin_nonce),
        }
    }
}

impl UnifiedTransferId {
    pub fn is_utxo(&self) -> bool {
        matches!(self.kind, TransferIdKind::Utxo(_))
    }
}

impl TryInto<TransferId> for &UnifiedTransferId {
    type Error = &'static str;

    fn try_into(self) -> Result<TransferId, Self::Error> {
        let origin_nonce = match self.kind {
            TransferIdKind::Nonce(nonce) => nonce,
            TransferIdKind::Utxo(_) => {
                return Err("Cannot convert UTXO transfer ID to general transfer ID")
            }
        };
        Ok(TransferId {
            origin_chain: self.origin_chain,
            origin_nonce,
        })
    }
}

impl std::fmt::Display for UnifiedTransferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            TransferIdKind::Nonce(nonce) => write!(f, "{}:{}", self.origin_chain.as_ref(), nonce),
            TransferIdKind::Utxo(utxo_id) => {
                write!(f, "{}:{}", self.origin_chain.as_ref(), utxo_id)
            }
        }
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FastTransferId(pub [u8; 32]);

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FastTransfer {
    pub transfer_id: UnifiedTransferId,
    pub token_id: AccountId,
    pub amount: U128,
    pub fee: Fee,
    pub recipient: OmniAddress,
    pub msg: String,
}

impl FastTransfer {
    #[allow(clippy::missing_panics_doc)]
    pub fn id(&self) -> FastTransferId {
        FastTransferId(utils::sha256(&borsh::to_vec(self).unwrap()))
    }
}

impl FastTransfer {
    pub fn from_transfer(transfer: TransferMessage, token_id: AccountId) -> Self {
        Self {
            transfer_id: UnifiedTransferId {
                origin_chain: transfer.get_origin_chain(),
                kind: TransferIdKind::Nonce(transfer.origin_nonce),
            },
            token_id,
            amount: transfer.amount,
            fee: transfer.fee,
            recipient: transfer.recipient,
            msg: transfer.msg,
        }
    }

    pub fn from_utxo_transfer(
        transfer: UtxoFinTransferMsg,
        token_id: AccountId,
        amount: U128,
        origin_chain: ChainKind,
    ) -> Self {
        Self {
            transfer_id: UnifiedTransferId {
                origin_chain,
                kind: TransferIdKind::Utxo(transfer.utxo_id),
            },
            token_id,
            amount,
            fee: Fee {
                fee: transfer.relayer_fee,
                native_fee: U128(0),
            },
            recipient: transfer.recipient,
            msg: transfer.msg,
        }
    }
}

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FastTransferStatus {
    pub finalised: bool,
    pub relayer: AccountId,
    pub storage_owner: AccountId,
}

#[near(serializers=[json])]
#[derive(Debug, PartialEq)]
pub enum DestinationChainMsg {
    MaxGasFee(U64),
    DestHexMsg(#[serde_as(as = "Hex")] Vec<u8>),
}

impl DestinationChainMsg {
    pub fn max_gas_fee(&self) -> Option<U128> {
        if let Self::MaxGasFee(fee) = self {
            Some(U128(fee.0.into()))
        } else {
            None
        }
    }

    pub fn destination_msg(&self) -> Option<Vec<u8>> {
        if let Self::DestHexMsg(msg) = self {
            Some(msg.clone())
        } else {
            None
        }
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}
