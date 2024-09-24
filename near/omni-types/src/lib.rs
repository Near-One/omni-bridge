use core::fmt;
use core::str::FromStr;
use hex::FromHex;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;
use serde::de::Visitor;

pub mod evm;
pub mod locker_args;
pub mod mpc_types;
pub mod near_events;
pub mod prover_args;
pub mod prover_result;

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
}

impl From<&OmniAddress> for ChainKind {
    fn from(input: &OmniAddress) -> Self {
        match input {
            OmniAddress::Eth(_) => ChainKind::Eth,
            OmniAddress::Near(_) => ChainKind::Near,
            OmniAddress::Sol(_) => ChainKind::Sol,
        }
    }
}

pub type EvmAddress = H160;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OmniAddress {
    Eth(EvmAddress),
    Near(String),
    Sol(String),
}

impl OmniAddress {
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
        }
    }
}

impl FromStr for OmniAddress {
    type Err = String;

    fn from_str(input: &str) -> Result<OmniAddress, Self::Err> {
        let (chain, recipient) = input.split_once(':').ok_or("Invalid OmniAddress format")?;

        match chain {
            "eth" => Ok(OmniAddress::Eth(recipient.parse().map_err(stringify)?)),
            "near" => Ok(OmniAddress::Near(recipient.to_owned())),
            "sol" => Ok(OmniAddress::Sol(recipient.to_owned())), // TODO validate sol address
            _ => Err(format!("Chain {chain} is not supported")),
        }
    }
}

impl fmt::Display for OmniAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (chain_str, recipient) = match self {
            OmniAddress::Eth(recipient) => ("eth", recipient.to_string()),
            OmniAddress::Near(recipient) => ("near", recipient.to_string()),
            OmniAddress::Sol(recipient) => ("sol", recipient.clone()),
        };
        write!(f, "{}:{}", chain_str, recipient)
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

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TransferMessage {
    pub origin_nonce: U128,
    pub token: AccountId,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee: U128,
    pub sender: OmniAddress,
}

impl TransferMessage {
    pub fn get_origin_chain(&self) -> ChainKind {
        self.sender.get_chain()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TransferMessagePayload {
    pub nonce: U128,
    pub token: AccountId,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub fee_recipient: Option<AccountId>,
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
    Fee(U128),
    Proof(Vec<u8>),
}

#[derive(Debug, Eq, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MetadataPayload {
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

pub type Nonce = u128;

pub fn stringify<T: std::fmt::Display>(item: T) -> String {
    item.to_string()
}
