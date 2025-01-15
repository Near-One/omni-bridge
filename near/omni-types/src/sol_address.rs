use core::fmt;
use core::str::FromStr;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::bs58;
use near_sdk::serde::{Deserialize, Serialize};
use serde::de::Visitor;

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Eq)]
pub struct SolAddress(pub [u8; 32]);

impl SolAddress {
    pub const ZERO: Self = Self([0u8; 32]);

    pub fn is_zero(&self) -> bool {
        *self == Self::ZERO
    }
}

impl FromStr for SolAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let result = bs58::decode(s).into_vec().map_err(|err| err.to_string())?;

        Ok(SolAddress(
            result
                .try_into()
                .map_err(|err| format!("Invalid length: {err:?}"))?,
        ))
    }
}

impl fmt::Display for SolAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", bs58::encode(self.0).into_string())
    }
}

impl<'de> Deserialize<'de> for SolAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Base58Visitor;

        impl<'de> Visitor<'de> for Base58Visitor {
            type Value = SolAddress;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a base58 string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Base58Visitor)
    }
}

impl Serialize for SolAddress {
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
