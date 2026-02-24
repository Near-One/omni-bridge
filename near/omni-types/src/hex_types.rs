use core::fmt;
use core::str::FromStr;

use hex::FromHex;
use near_sdk::near;
use near_sdk::serde::{Deserialize, Serialize};
use serde::de::Visitor;

use crate::errors::TypesError;

// Macro to generate common implementations for hash types (H160, H256, etc.)
macro_rules! impl_h_type {
    ($name:ident, $size:expr, $padded:expr) => {
        #[near(serializers = [borsh])]
        #[derive(Debug, Clone, Hash, PartialEq, Eq)]
        pub struct $name(pub [u8; $size]);

        impl FromStr for $name {
            type Err = TypesError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let hex_str = s.strip_prefix("0x").unwrap_or(s);
                if hex_str.len() > $size * 2 {
                    return Err(TypesError::InvalidHexLength);
                }

                let hex_str = if $padded {
                    &format!("{:0>width$}", hex_str, width = $size * 2)
                } else {
                    hex_str
                };

                let result = Vec::from_hex(&hex_str).map_err(|_| TypesError::InvalidHex)?;
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

        impl From<[u8; $size]> for $name {
            fn from(bytes: [u8; $size]) -> Self {
                Self(bytes)
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
impl_h_type!(H160, 20, false);
impl_h_type!(H256, 32, true);

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn test_h160() {
        // Parse with and without 0x prefix
        let addr = H160::from_str("0x23ddd3e3692d1861ed57ede224608875809e127f").unwrap();
        assert_eq!(
            addr.to_string(),
            "0x23ddd3e3692d1861ed57ede224608875809e127f"
        );
        assert_eq!(
            addr,
            H160::from_str("23ddd3e3692d1861ed57ede224608875809e127f").unwrap()
        );

        // Rejects short hex (strict, no padding)
        assert!(H160::from_str("0x1234").is_err());

        // Rejects too-long hex
        let err = H160::from_str(&format!("0x{}", "aa".repeat(21))).unwrap_err();
        assert_eq!(err, TypesError::InvalidHexLength);

        // Rejects invalid hex chars
        let err = H160::from_str(&format!("0x{}gg", "00".repeat(19))).unwrap_err();
        assert_eq!(err, TypesError::InvalidHex);

        // Zero constant
        let zero = H160::ZERO;
        assert!(zero.is_zero());
        assert_eq!(zero.to_string(), format!("0x{}", "00".repeat(20)));
        assert!(!addr.is_zero());

        // From bytes
        let bytes = [0xab; 20];
        assert_eq!(H160::from(bytes).0, bytes);

        // Display roundtrip
        let reparsed = H160::from_str(&addr.to_string()).unwrap();
        assert_eq!(addr, reparsed);

        // Serde roundtrip
        let json = near_sdk::serde_json::to_string(&addr).unwrap();
        let deserialized: H160 = near_sdk::serde_json::from_str(&json).unwrap();
        assert_eq!(addr, deserialized);
    }

    #[test]
    fn test_h256() {
        // Parse with and without 0x prefix
        let hash =
            H256::from_str("0x05558831a603eca8cd69a42d4251f08de3573039b69f23972265cac76639f1cf")
                .unwrap();
        assert_eq!(
            hash.to_string(),
            "0x05558831a603eca8cd69a42d4251f08de3573039b69f23972265cac76639f1cf"
        );
        assert_eq!(
            hash,
            H256::from_str("05558831a603eca8cd69a42d4251f08de3573039b69f23972265cac76639f1cf")
                .unwrap()
        );

        // Pads short hex with leading zeros
        let short = H256::from_str("0x1").unwrap();
        assert_eq!(short.0[31], 1);
        assert!(short.0[..31].iter().all(|&b| b == 0));

        // Pads empty to zero
        let empty = H256::from_str("0x").unwrap();
        assert!(empty.is_zero());

        // Rejects too-long hex
        let err = H256::from_str(&format!("0x{}", "aa".repeat(33))).unwrap_err();
        assert_eq!(err, TypesError::InvalidHexLength);

        // Rejects invalid hex chars
        let err = H256::from_str(&format!("0x{}zz", "00".repeat(31))).unwrap_err();
        assert_eq!(err, TypesError::InvalidHex);

        // Zero constant
        let zero = H256::ZERO;
        assert!(zero.is_zero());
        assert_eq!(zero.to_string(), format!("0x{}", "00".repeat(32)));

        // From bytes
        let bytes = [0xcd; 32];
        assert_eq!(H256::from(bytes).0, bytes);

        // Display roundtrip
        let reparsed = H256::from_str(&hash.to_string()).unwrap();
        assert_eq!(hash, reparsed);

        // Serde roundtrip
        let json = near_sdk::serde_json::to_string(&hash).unwrap();
        let deserialized: H256 = near_sdk::serde_json::from_str(&json).unwrap();
        assert_eq!(hash, deserialized);
    }
}
