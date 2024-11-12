use core::fmt;
use core::str::FromStr;
use hex::FromHex;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;
use serde::de::Visitor;
use sol_address::SolAddress;

pub mod evm;
pub mod locker_args;
pub mod mpc_types;
pub mod near_events;
pub mod prover_args;
pub mod prover_result;
pub mod sol_address;

#[cfg(test)]
mod tests;

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub struct H160(pub [u8; 20]);

impl FromStr for H160 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = if let Some(stripped) = s.strip_prefix("0x") {
            stripped
        } else {
            s
        };
        let result = Vec::from_hex(s).map_err(|err| err.to_string())?;
        Ok(H160(
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

        let hash = evm::utils::keccak256(hex_addr.as_bytes());

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

        impl<'de> Visitor<'de> for HexVisitor {
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

#[derive(
    Debug,
    Eq,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    Ord,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
    strum_macros::AsRefStr,
)]
pub enum ChainKind {
    Eth,
    Near,
    Sol,
    Arb,
    Base,
}

impl From<&OmniAddress> for ChainKind {
    fn from(input: &OmniAddress) -> Self {
        input.get_chain()
    }
}

pub type EvmAddress = H160;

pub const ZERO_ACCOUNT_ID: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub enum OmniAddress {
    Eth(EvmAddress),
    Near(AccountId),
    Sol(SolAddress),
    Arb(EvmAddress),
    Base(EvmAddress),
}

impl OmniAddress {
    #[allow(clippy::missing_panics_doc)]
    pub fn new_zero(chain_kind: ChainKind) -> OmniAddress {
        match chain_kind {
            ChainKind::Eth => OmniAddress::Eth(H160::ZERO),
            ChainKind::Near => OmniAddress::Near(ZERO_ACCOUNT_ID.parse().unwrap()),
            ChainKind::Sol => OmniAddress::Sol(SolAddress::ZERO),
            ChainKind::Arb => OmniAddress::Arb(H160::ZERO),
            ChainKind::Base => OmniAddress::Base(H160::ZERO),
        }
    }

    pub fn from_evm_address(chain_kind: ChainKind, address: EvmAddress) -> Result<Self, String> {
        match chain_kind {
            ChainKind::Eth => Ok(Self::Eth(address)),
            _ => Err(format!("{chain_kind:?} is not an EVM chain")),
        }
    }

    pub fn get_chain(&self) -> ChainKind {
        match self {
            OmniAddress::Eth(_) => ChainKind::Eth,
            OmniAddress::Near(_) => ChainKind::Near,
            OmniAddress::Sol(_) => ChainKind::Sol,
            OmniAddress::Arb(_) => ChainKind::Arb,
            OmniAddress::Base(_) => ChainKind::Base,
        }
    }

    pub fn encode(&self, separator: char, skip_zero_address: bool) -> String {
        let (chain_str, address) = match self {
            OmniAddress::Eth(address) => ("eth", address.to_string()),
            OmniAddress::Near(address) => ("near", address.to_string()),
            OmniAddress::Sol(address) => ("sol", address.to_string()),
            OmniAddress::Arb(address) => ("arb", address.to_string()),
            OmniAddress::Base(address) => ("base", address.to_string()),
        };

        if skip_zero_address && self.is_zero() {
            chain_str.to_string()
        } else {
            format!("{chain_str}{separator}{address}")
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            OmniAddress::Eth(address) | OmniAddress::Arb(address) | OmniAddress::Base(address) => {
                address.is_zero()
            }
            OmniAddress::Near(address) => *address == ZERO_ACCOUNT_ID,
            OmniAddress::Sol(address) => address.is_zero(),
        }
    }

    pub fn get_token_prefix(&self) -> String {
        self.encode('-', true)
    }
}

impl FromStr for OmniAddress {
    type Err = String;

    fn from_str(input: &str) -> Result<OmniAddress, Self::Err> {
        let (chain, recipient) = input.split_once(':').unwrap_or(("eth", input));

        match chain {
            "eth" => Ok(OmniAddress::Eth(recipient.parse().map_err(stringify)?)),
            "near" => Ok(OmniAddress::Near(recipient.parse().map_err(stringify)?)),
            "sol" => Ok(OmniAddress::Sol(recipient.parse().map_err(stringify)?)),
            "arb" => Ok(OmniAddress::Arb(recipient.parse().map_err(stringify)?)),
            "base" => Ok(OmniAddress::Base(recipient.parse().map_err(stringify)?)),
            _ => Err(format!("Chain {chain} is not supported")),
        }
    }
}

impl fmt::Display for OmniAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.encode(':', false))
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

        impl<'de> serde::de::Visitor<'de> for OmniAddressVisitor {
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

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct NearRecipient {
    pub target: AccountId,
    pub message: Option<String>,
}

impl FromStr for NearRecipient {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (target, message) = input.split_once(':').map_or_else(
            || (input, None),
            |(recipient, msg)| (recipient, Some(msg.to_owned())),
        );

        Ok(Self {
            target: target.parse().map_err(stringify)?,
            message,
        })
    }
}

impl fmt::Display for NearRecipient {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(message) = &self.message {
            write!(f, "{}:{}", self.target, message)
        } else {
            write!(f, "{}", self.target)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitTransferMsg {
    pub recipient: OmniAddress,
    pub fee: U128,
    pub native_token_fee: U128,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct FeeRecipient {
    pub recipient: AccountId,
    pub native_fee_recipient: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct NativeFee {
    pub amount: U128,
    pub recipient: OmniAddress,
}

#[derive(
    BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, PartialEq, Default,
)]
pub struct Fee {
    pub fee: U128,
    pub native_fee: U128,
}

impl Fee {
    pub fn is_zero(&self) -> bool {
        self.fee.0 == 0 && self.native_fee.0 == 0
    }
}

pub type TransferId = (ChainKind, Nonce);
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TransferMessage {
    pub origin_nonce: U128,
    pub token: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: Fee,
    pub sender: OmniAddress,
    pub msg: String,
}

impl TransferMessage {
    pub fn get_origin_chain(&self) -> ChainKind {
        self.sender.get_chain()
    }

    pub fn get_transfer_id(&self) -> TransferId {
        (self.get_origin_chain(), self.origin_nonce.0)
    }

    pub fn get_destination_chain(&self) -> ChainKind {
        self.recipient.get_chain()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub enum PayloadType {
    TransferMessage,
    Metadata,
    ClaimNativeFee,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TransferMessagePayload {
    pub prefix: PayloadType,
    pub nonce: U128,
    pub token_address: OmniAddress,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee_recipient: Option<AccountId>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct ClaimNativeFeePayload {
    pub prefix: PayloadType,
    pub nonces: Vec<U128>,
    pub amount: U128,
    pub recipient: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct MetadataPayload {
    pub prefix: PayloadType,
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SignRequest {
    pub payload: [u8; 32],
    pub path: String,
    pub key_version: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub enum UpdateFee {
    Fee(Fee),
    Proof(Vec<u8>),
}

pub type Nonce = u128;

pub fn stringify<T: std::fmt::Display>(item: T) -> String {
    item.to_string()
}

#[derive(Deserialize, Serialize, Clone)]
pub struct BasicMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}
