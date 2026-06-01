use std::fmt;
use std::io;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};

use crate::errors::TypesError;

/// UTF-8 string whose byte length is bounded by `MAX`.
///
/// The bound is enforced on construction ([`BoundedString::new`] / [`FromStr`]) and on
/// both JSON and Borsh deserialization. Oversized inputs fail with
/// [`TypesError::StringTooLong`], preventing untrusted callers from passing arbitrarily
/// large strings that would inflate storage and gas costs or grow hashes without bound.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, BorshSerialize)]
#[serde(transparent)]
pub struct BoundedString<const MAX: usize>(String);

impl<const MAX: usize> BoundedString<MAX> {
    pub const MAX_LEN: usize = MAX;

    pub fn new(s: impl Into<String>) -> Result<Self, TypesError> {
        let s = s.into();
        if s.len() > MAX {
            return Err(TypesError::StringTooLong);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> fmt::Display for BoundedString<MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const MAX: usize> AsRef<str> for BoundedString<MAX> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> From<BoundedString<MAX>> for String {
    fn from(value: BoundedString<MAX>) -> Self {
        value.0
    }
}

impl<const MAX: usize> FromStr for BoundedString<MAX> {
    type Err = TypesError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for BoundedString<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> BorshDeserialize for BoundedString<MAX> {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let s = String::deserialize_reader(reader)?;
        Self::new(s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Bs8 = BoundedString<8>;

    #[test]
    fn new_accepts_within_limit() {
        assert_eq!(Bs8::new("hello").unwrap().as_str(), "hello");
        // Exact limit is allowed.
        assert_eq!(Bs8::new("12345678").unwrap().as_str(), "12345678");
        // Empty is allowed.
        assert_eq!(Bs8::new("").unwrap().as_str(), "");
    }

    #[test]
    fn new_rejects_oversize() {
        let err = Bs8::new("123456789").unwrap_err();
        assert_eq!(err, TypesError::StringTooLong);
    }

    #[test]
    fn json_roundtrips_and_enforces_limit() {
        let bs = Bs8::new("hi").unwrap();
        let json = near_sdk::serde_json::to_string(&bs).unwrap();
        assert_eq!(json, "\"hi\"");
        let parsed: Bs8 = near_sdk::serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, bs);

        // Oversize rejected at deserialization.
        let err = near_sdk::serde_json::from_str::<Bs8>("\"123456789\"").unwrap_err();
        assert!(err.to_string().contains("ERR_STRING_TOO_LONG"));
    }

    #[test]
    fn borsh_roundtrips_and_enforces_limit() {
        let bs = Bs8::new("hi").unwrap();
        let bytes = borsh::to_vec(&bs).unwrap();
        let parsed: Bs8 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(parsed, bs);

        // Manually craft a borsh-encoded oversized string and confirm it's rejected.
        let oversized = String::from("123456789");
        let bytes = borsh::to_vec(&oversized).unwrap();
        assert!(borsh::from_slice::<Bs8>(&bytes).is_err());
    }
}
