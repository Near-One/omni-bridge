use core::str::FromStr;
use core::{fmt, str};
use hex::FromHex;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::AccountId;
use serde::de::Visitor;

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
                Ok(s.parse().map_err(serde::de::Error::custom)?)
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

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
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

pub type EthAddress = H160;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OmniAddress {
    Eth(EthAddress),
    Near(String),
    Sol(String),
}

impl OmniAddress {
    pub fn get_chain(&self) -> ChainKind {
        match self {
            OmniAddress::Eth(_) => ChainKind::Eth,
            OmniAddress::Near(_) => ChainKind::Near,
            OmniAddress::Sol(_) => ChainKind::Sol,
        }
    }
}

impl FromStr for OmniAddress {
    type Err = ();

    fn from_str(input: &str) -> Result<OmniAddress, Self::Err> {
        let (chain, recipient) = input.split_once(':').ok_or(())?;

        match chain {
            "eth" => Ok(OmniAddress::Eth(recipient.parse().map_err(|_| ())?)),
            "near" => Ok(OmniAddress::Near(recipient.to_owned())),
            "sol" => Ok(OmniAddress::Sol(recipient.to_owned())), // TODO validate sol address
            _ => Err(()),
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
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (target, message) = input.split_once(':').map_or_else(
            || (input, None),
            |(recipient, msg)| (recipient, Some(msg.to_owned())),
        );

        Ok(Self {
            target: target.parse().map_err(|_| ())?,
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
pub struct FinTransferMessage {
    pub nonce: U128,
    pub claim_recipient: AccountId,
    pub factory: OmniAddress,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub enum ProofResult {
    InitTransfer(TransferMessage),
    FinTransfer(FinTransferMessage),
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, Clone)]
pub struct TransferMessagePayload {
    pub nonce: U128,
    pub token: AccountId,
    pub amount: U128,
    pub recipient: OmniAddress,
    pub relayer: Option<OmniAddress>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SignRequest {
    pub payload: [u8; 32],
    pub path: String,
    pub key_version: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AffinePoint {
    pub affine_point: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Scalar {
    pub scalar: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SignatureResponse {
    pub big_r: AffinePoint,
    pub s: Scalar,
    pub recovery_id: u8,
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
