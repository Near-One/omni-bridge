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
#[derive(
    Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, BorshSerialize,
)]
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

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
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

impl<const MAX: usize> PartialEq<str> for BoundedString<MAX> {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl<const MAX: usize> PartialEq<&str> for BoundedString<MAX> {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl<const MAX: usize> PartialEq<String> for BoundedString<MAX> {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
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

/// Returns `true` for bytes that `serde_json` emits verbatim inside a string literal.
///
/// Printable ASCII minus `"` and `\` — exactly the bytes that are neither escaped as a
/// two-character sequence (`\"`, `\\`, `\n`, …) nor as a six-character `\u00XX` escape.
/// A string made only of these bytes JSON-encodes to its own byte length plus the two
/// surrounding quotes, which is what makes [`PlainString`]'s log bound tight.
const fn is_json_plain(b: u8) -> bool {
    b >= 0x20 && b <= 0x7E && b != b'"' && b != b'\\'
}

/// Byte-bounded string additionally restricted to characters that JSON never escapes.
///
/// [`BoundedString`] bounds the *input* length, but a string of `MAX` control characters
/// JSON-encodes to `6 * MAX + 2` bytes, so a generous input bound still implies a huge log
/// bound. `PlainString` rejects anything outside printable ASCII (and `"`/`\`), pinning the
/// JSON expansion factor at exactly 1 — so its log bound is `MAX + 2`.
///
/// Use it for identifiers drawn from a known alphabet: base58/bech32 chain addresses,
/// hex transaction hashes. Use [`BoundedString`] for free-form user payloads.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, BorshSerialize,
)]
#[serde(transparent)]
pub struct PlainString<const MAX: usize>(String);

impl<const MAX: usize> PlainString<MAX> {
    pub const MAX_LEN: usize = MAX;

    pub fn new(s: impl Into<String>) -> Result<Self, TypesError> {
        let s = s.into();
        if s.len() > MAX {
            return Err(TypesError::StringTooLong);
        }
        if !s.bytes().all(is_json_plain) {
            return Err(TypesError::NonPlainString);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<const MAX: usize> fmt::Display for PlainString<MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const MAX: usize> AsRef<str> for PlainString<MAX> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<const MAX: usize> From<PlainString<MAX>> for String {
    fn from(value: PlainString<MAX>) -> Self {
        value.0
    }
}

impl<const MAX: usize> FromStr for PlainString<MAX> {
    type Err = TypesError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<const MAX: usize> PartialEq<str> for PlainString<MAX> {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl<const MAX: usize> PartialEq<&str> for PlainString<MAX> {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl<const MAX: usize> PartialEq<String> for PlainString<MAX> {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for PlainString<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> BorshDeserialize for PlainString<MAX> {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let s = String::deserialize_reader(reader)?;
        Self::new(s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

/// Byte vector whose length is bounded by `MAX`.
///
/// Borsh- and JSON-compatible with a plain `Vec<u8>` (the JSON form is an array of
/// integers), so swapping a `Vec<u8>` field for this type does not change any wire
/// encoding or signature payload — it only adds the bound.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, BorshSerialize,
)]
#[serde(transparent)]
pub struct BoundedBytes<const MAX: usize>(Vec<u8>);

impl<const MAX: usize> BoundedBytes<MAX> {
    pub const MAX_LEN: usize = MAX;

    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, TypesError> {
        let bytes = bytes.into();
        if bytes.len() > MAX {
            return Err(TypesError::BytesTooLong);
        }
        Ok(Self(bytes))
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<const MAX: usize> AsRef<[u8]> for BoundedBytes<MAX> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<const MAX: usize> From<BoundedBytes<MAX>> for Vec<u8> {
    fn from(value: BoundedBytes<MAX>) -> Self {
        value.0
    }
}

impl<'de, const MAX: usize> Deserialize<'de> for BoundedBytes<MAX> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = <Vec<u8> as Deserialize>::deserialize(deserializer)?;
        Self::new(bytes).map_err(serde::de::Error::custom)
    }
}

impl<const MAX: usize> BorshDeserialize for BoundedBytes<MAX> {
    fn deserialize_reader<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let bytes = Vec::<u8>::deserialize_reader(reader)?;
        Self::new(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

/// Schema impls for the bounded wrappers.
///
/// Each delegates to the type it wraps, so ABI generation produces exactly the schema it did
/// before these fields were bounded — the cap is a runtime/compile-time invariant, not a
/// wire-format change. `BorshSchema` is gated on non-wasm to match what `#[near]` emits.
mod schema_impls {
    use super::{BoundedBytes, BoundedString, PlainString};

    macro_rules! delegate_json_schema {
        ($ty:ident, $inner:ty) => {
            impl<const MAX: usize> schemars::JsonSchema for $ty<MAX> {
                fn is_referenceable() -> bool {
                    false
                }

                fn schema_name() -> String {
                    <$inner as schemars::JsonSchema>::schema_name()
                }

                fn json_schema(
                    gen: &mut schemars::gen::SchemaGenerator,
                ) -> schemars::schema::Schema {
                    <$inner as schemars::JsonSchema>::json_schema(gen)
                }
            }
        };
    }

    delegate_json_schema!(BoundedString, String);
    delegate_json_schema!(PlainString, String);
    delegate_json_schema!(BoundedBytes, Vec<u8>);

    #[cfg(not(target_arch = "wasm32"))]
    macro_rules! delegate_borsh_schema {
        ($ty:ident, $inner:ty) => {
            impl<const MAX: usize> borsh::BorshSchema for $ty<MAX> {
                fn declaration() -> borsh::schema::Declaration {
                    <$inner as borsh::BorshSchema>::declaration()
                }

                fn add_definitions_recursively(
                    definitions: &mut std::collections::BTreeMap<
                        borsh::schema::Declaration,
                        borsh::schema::Definition,
                    >,
                ) {
                    <$inner as borsh::BorshSchema>::add_definitions_recursively(definitions);
                }
            }
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    delegate_borsh_schema!(BoundedString, String);
    #[cfg(not(target_arch = "wasm32"))]
    delegate_borsh_schema!(PlainString, String);
    #[cfg(not(target_arch = "wasm32"))]
    delegate_borsh_schema!(BoundedBytes, Vec<u8>);
}

#[cfg(test)]
mod tests {
    use super::*;

    type Bs8 = BoundedString<8>;
    type Ps8 = PlainString<8>;
    type Bb4 = BoundedBytes<4>;

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

    #[test]
    fn plain_string_accepts_printable_ascii() {
        assert_eq!(Ps8::new("abc-123").unwrap().as_str(), "abc-123");
        assert_eq!(Ps8::new("").unwrap().as_str(), "");
    }

    #[test]
    fn plain_string_rejects_escaped_and_oversize() {
        assert_eq!(Ps8::new("a\"b").unwrap_err(), TypesError::NonPlainString);
        assert_eq!(Ps8::new("a\\b").unwrap_err(), TypesError::NonPlainString);
        assert_eq!(Ps8::new("a\nb").unwrap_err(), TypesError::NonPlainString);
        assert_eq!(Ps8::new("a\u{1}b").unwrap_err(), TypesError::NonPlainString);
        assert_eq!(Ps8::new("é").unwrap_err(), TypesError::NonPlainString);
        assert_eq!(
            Ps8::new("123456789").unwrap_err(),
            TypesError::StringTooLong
        );
    }

    /// The property [`PlainString`]'s log bound rests on: JSON encoding never expands it.
    #[test]
    fn plain_string_json_never_expands() {
        for b in 0x20u8..=0x7E {
            if b == b'"' || b == b'\\' {
                continue;
            }
            let s = PlainString::<8>::new(String::from(b as char)).unwrap();
            let json = near_sdk::serde_json::to_string(&s).unwrap();
            assert_eq!(json.len(), s.len() + 2, "byte {b:#04x} expanded under JSON");
        }
    }

    #[test]
    fn bounded_bytes_enforces_limit_and_wire_compat() {
        let bb = Bb4::new(vec![1u8, 2, 3]).unwrap();
        assert_eq!(bb.as_slice(), &[1, 2, 3]);
        assert_eq!(
            Bb4::new(vec![0u8; 5]).unwrap_err(),
            TypesError::BytesTooLong
        );

        // Borsh and JSON encodings are byte-identical to a plain `Vec<u8>`.
        let raw = vec![1u8, 2, 3];
        assert_eq!(borsh::to_vec(&bb).unwrap(), borsh::to_vec(&raw).unwrap());
        assert_eq!(
            near_sdk::serde_json::to_string(&bb).unwrap(),
            near_sdk::serde_json::to_string(&raw).unwrap()
        );

        // Oversized values are rejected on both decoders.
        let oversized = borsh::to_vec(&vec![0u8; 5]).unwrap();
        assert!(borsh::from_slice::<Bb4>(&oversized).is_err());
        assert!(near_sdk::serde_json::from_str::<Bb4>("[0,0,0,0,0]").is_err());
    }
}
