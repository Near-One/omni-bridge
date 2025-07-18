use borsh::{BorshDeserialize, BorshSerialize};
use core::fmt;
use core::str::FromStr;
use hex::FromHex;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{near, AccountId};
use num_enum::IntoPrimitive;
use schemars::JsonSchema;
use serde::de::Visitor;
use sol_address::SolAddress;

pub mod evm;
pub mod locker_args;
pub mod mpc_types;
pub mod near_events;
pub mod prover_args;
pub mod prover_result;
pub mod sol_address;
pub mod utils;

#[cfg(test)]
mod tests;

#[near(serializers = [borsh])]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct H160(pub [u8; 20]);

impl FromStr for H160 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = Vec::from_hex(s.strip_prefix("0x").map_or(s, |stripped| stripped))
            .map_err(|_| "ERR_INVALIDE_HEX")?;
        Ok(Self(
            result
                .try_into()
                .map_err(|err| format!("Invalid length: {err:?}"))?,
        ))
    }
}

impl fmt::Display for H160 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl H160 {
    pub const ZERO: Self = Self([0u8; 20]);

    pub fn is_zero(&self) -> bool {
        *self == Self::ZERO
    }

    pub fn to_eip_55_checksum(&self) -> String {
        let hex_addr = hex::encode(self.0);

        let hash = utils::keccak256(hex_addr.as_bytes());

        let mut result = String::with_capacity(40);

        for (i, c) in hex_addr.chars().enumerate() {
            let hash_byte = hash[i / 2];

            let hash_nibble = if i % 2 == 0 {
                (hash_byte >> 4) & 0xF
            } else {
                hash_byte & 0xF
            };

            let c = match c {
                'a'..='f' => {
                    if hash_nibble >= 8 {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    }
                }
                _ => c,
            };

            result.push(c);
        }

        result
    }
}

impl<'de> Deserialize<'de> for H160 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexVisitor;

        impl Visitor<'_> for HexVisitor {
            type Value = H160;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(HexVisitor)
    }
}

impl Serialize for H160 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

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
            _ => Err(format!("{input:?} invalid chain kind")),
        }
    }
}

pub type EvmAddress = H160;

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
            _ => Err(format!("{chain_kind:?} is not an EVM chain")),
        }
    }

    pub fn new_from_slice(chain_kind: ChainKind, address: &[u8]) -> Result<Self, String> {
        match chain_kind {
            ChainKind::Sol => Ok(Self::Sol(Self::to_sol_address(address)?)),
            ChainKind::Eth | ChainKind::Arb | ChainKind::Base => {
                Self::new_from_evm_address(chain_kind, Self::to_evm_address(address)?)
            }
            ChainKind::Near => Ok(Self::Near(Self::to_near_account_id(address)?)),
        }
    }

    pub const fn get_chain(&self) -> ChainKind {
        match self {
            Self::Eth(_) => ChainKind::Eth,
            Self::Near(_) => ChainKind::Near,
            Self::Sol(_) => ChainKind::Sol,
            Self::Arb(_) => ChainKind::Arb,
            Self::Base(_) => ChainKind::Base,
        }
    }

    pub fn encode(&self, separator: char, skip_zero_address: bool) -> String {
        let (chain_str, address) = match self {
            Self::Eth(address) => ("eth", address.to_string()),
            Self::Near(address) => ("near", address.to_string()),
            Self::Sol(address) => ("sol", address.to_string()),
            Self::Arb(address) => ("arb", address.to_string()),
            Self::Base(address) => ("base", address.to_string()),
        };

        if skip_zero_address && self.is_zero() {
            chain_str.to_string()
        } else {
            format!("{chain_str}{separator}{address}")
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Self::Eth(address) | Self::Arb(address) | Self::Base(address) => address.is_zero(),
            Self::Near(address) => *address == ZERO_ACCOUNT_ID,
            Self::Sol(address) => address.is_zero(),
        }
    }

    pub fn get_token_prefix(&self) -> String {
        match self {
            Self::Sol(address) => {
                if self.is_zero() {
                    "sol".to_string()
                } else {
                    // The AccountId on Near can't be uppercased and has a 64 character limit,
                    // so we encode the solana address into 20 bytes to bypass these restrictions
                    let hashed_address = H160(
                        utils::keccak256(&address.0)[12..]
                            .try_into()
                            .unwrap_or_default(),
                    )
                    .to_string();
                    format!("sol-{hashed_address}")
                }
            }
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
}

#[derive(Serialize, Deserialize, BorshSerialize, BorshDeserialize, Debug, Clone)]
pub struct FastFinTransferMsg {
    pub transfer_id: TransferId,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub msg: String,
    pub amount: U128,
    pub storage_deposit_amount: Option<U128>,
    pub relayer: AccountId,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitTransferMsg {
    pub recipient: OmniAddress,
    pub fee: U128,
    pub native_token_fee: U128,
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
pub struct TransferMessageV0 {
    pub origin_nonce: Nonce,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
    pub destination_nonce: Nonce,
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
    pub origin_transfer_id: Option<TransferId>,
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
pub struct TransferMessagePayload {
    pub prefix: PayloadType,
    pub destination_nonce: Nonce,
    pub transfer_id: TransferId,
    pub token_address: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee_recipient: Option<AccountId>,
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
#[derive(Debug, Clone)]
pub struct FastTransferId(pub [u8; 32]);

#[near(serializers=[borsh, json])]
#[derive(Debug, Clone)]
pub struct FastTransfer {
    pub transfer_id: TransferId,
    pub token_id: AccountId,
    pub amount: U128,
    pub fee: Fee,
    pub recipient: OmniAddress,
    pub msg: String,
}

impl FastTransfer {
    #[allow(clippy::missing_panics_doc)]
    pub fn id(&self) -> FastTransferId {
        FastTransferId(near_sdk::env::sha256_array(&borsh::to_vec(self).unwrap()))
    }
}

impl FastTransfer {
    pub fn from_transfer(transfer: TransferMessage, token_id: AccountId) -> Self {
        Self {
            transfer_id: transfer.get_transfer_id(),
            token_id,
            amount: transfer.amount,
            fee: transfer.fee,
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
