use core::fmt;
use core::str::FromStr;

use hex::FromHex;
use near_sdk::near;
use near_sdk::serde::{Deserialize, Serialize};
use serde::de::Visitor;

use crate::errors::TypesError;

// Macro to generate common implementations for hash types (H160, H256, etc.)
macro_rules! impl_h_type {
    ($name:ident, $size:expr) => {
        #[near(serializers = [borsh])]
        #[derive(Debug, Clone, Hash, PartialEq, Eq)]
        pub struct $name(pub [u8; $size]);

        impl FromStr for $name {
            type Err = TypesError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let result = Vec::from_hex(s.strip_prefix("0x").map_or(s, |stripped| stripped))
                    .map_err(|_| TypesError::InvalidHex)?;
                Ok(Self(
                    result
                        .try_into()
                        .map_err(|_| TypesError::InvalidHexLength)?,
                ))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "0x{}", hex::encode(self.0))
            }
        }

        impl $name {
            pub const ZERO: Self = Self([0u8; $size]);

            pub fn is_zero(&self) -> bool {
                *self == Self::ZERO
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct HexVisitor;

                impl Visitor<'_> for HexVisitor {
                    type Value = $name;

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

        impl Serialize for $name {
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
    };
}

// Generate H160 (20 bytes) and H256 (32 bytes) implementations
impl_h_type!(H160, 20);
impl_h_type!(H256, 32);
