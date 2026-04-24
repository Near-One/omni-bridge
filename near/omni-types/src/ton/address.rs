//! TON address type + user-friendly base64 codec.
//!
//! Matches the format accepted by NEAR Intents
//! (<https://docs.near-intents.org/near-intents/chain-address-support>): 48-char
//! base64 (or base64url) of a 36-byte structure.
//!
//! ```text
//!   byte  0    : flags  — 0x11 EQ (bounceable) | 0x51 UQ (non-bounceable);
//!                high bits 0x80=testnet, 0x40=non-bounceable; low 6 bits = 0x11
//!   byte  1    : workchain (signed int8); only 0 (basechain) is accepted
//!   bytes 2-33 : account hash (uint256)
//!   bytes 34-35: CRC-16/XMODEM over bytes 0..34
//! ```
//!
//! On parse we accept any combination of the testnet (0x80) and non-bounceable
//! (0x40) flag bits as long as the low 6 bits are `0x11` — on-chain, only the
//! workchain + 32-byte hash matter, so discriminating by display flags would
//! reject otherwise-valid addresses. Masterchain (workchain != 0) is rejected
//! because the locker is basechain-only.
//!
//! On encode we always emit mainnet-bounceable-basechain (`EQ`-prefixed) using
//! base64url without padding.

use core::fmt;
use core::str::FromStr;

use near_sdk::near;
use near_sdk::serde::{Deserialize, Serialize};
use serde::de::Visitor;

const TON_USER_FRIENDLY_LEN: usize = 48;
const TAG_LOW_BITS: u8 = 0x11;
const TAG_TESTNET_BIT: u8 = 0x80;
const TAG_NON_BOUNCEABLE_BIT: u8 = 0x40;
const TAG_BOUNCEABLE_MAINNET: u8 = 0x11;

#[near(serializers = [borsh])]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TonAddress(pub [u8; 32]);

impl TonAddress {
    pub const ZERO: Self = Self([0u8; 32]);

    pub fn is_zero(&self) -> bool {
        *self == Self::ZERO
    }
}

impl FromStr for TonAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != TON_USER_FRIENDLY_LEN {
            return Err(format!(
                "TON address: expected {TON_USER_FRIENDLY_LEN} base64 chars, got {}",
                s.len()
            ));
        }
        let raw = decode_base64_36(s.as_bytes())
            .ok_or_else(|| "TON address: invalid base64 character".to_string())?;

        // The tag byte is a display-level hint: bit 0x80 = testnet,
        // bit 0x40 = non-bounceable, low 6 bits = 0x11. On-chain only the
        // workchain + 32-byte hash matter, so accept any combination of the
        // testnet / non-bounceable bits and validate the low bits only.
        let flags = raw[0];
        let low_bits = flags & !(TAG_TESTNET_BIT | TAG_NON_BOUNCEABLE_BIT);
        if low_bits != TAG_LOW_BITS {
            return Err(format!(
                "TON address: invalid flags byte {flags:#04x} (low bits {low_bits:#04x}, expected {TAG_LOW_BITS:#04x})"
            ));
        }

        let workchain = i8::from_ne_bytes([raw[1]]);
        if workchain != 0 {
            return Err(format!(
                "TON address: only basechain (workchain=0) is supported, got {workchain}"
            ));
        }

        let expected_crc = crc16_xmodem(&raw[0..34]);
        let got_crc = u16::from_be_bytes([raw[34], raw[35]]);
        if expected_crc != got_crc {
            return Err("TON address: CRC-16 mismatch".to_string());
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&raw[2..34]);
        Ok(Self(hash))
    }
}

impl fmt::Display for TonAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut raw = [0u8; 36];
        raw[0] = TAG_BOUNCEABLE_MAINNET;
        // raw[1] = 0 basechain
        raw[2..34].copy_from_slice(&self.0);
        let crc = crc16_xmodem(&raw[0..34]).to_be_bytes();
        raw[34] = crc[0];
        raw[35] = crc[1];
        f.write_str(&encode_base64url_36(&raw))
    }
}

impl<'de> Deserialize<'de> for TonAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StrVisitor;
        impl Visitor<'_> for StrVisitor {
            type Value = TonAddress;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a TON user-friendly base64 address (48 chars, EQ/UQ prefix)")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<TonAddress, E> {
                s.parse().map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(StrVisitor)
    }
}

impl Serialize for TonAddress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

fn decode_base64_36(input: &[u8]) -> Option<[u8; 36]> {
    debug_assert!(input.len() == 48);
    let mut out = [0u8; 36];
    for i in 0..12 {
        let a = b64_decode_char(input[i * 4])?;
        let b = b64_decode_char(input[i * 4 + 1])?;
        let c = b64_decode_char(input[i * 4 + 2])?;
        let d = b64_decode_char(input[i * 4 + 3])?;
        out[i * 3] = (a << 2) | (b >> 4);
        out[i * 3 + 1] = ((b & 0x0F) << 4) | (c >> 2);
        out[i * 3 + 2] = ((c & 0x03) << 6) | d;
    }
    Some(out)
}

fn b64_decode_char(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

const B64URL_ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_base64url_36(raw: &[u8; 36]) -> String {
    let mut out = String::with_capacity(48);
    for chunk in raw.chunks_exact(3) {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        out.push(char::from(B64URL_ALPHA[usize::from(b0 >> 2)]));
        out.push(char::from(
            B64URL_ALPHA[usize::from(((b0 & 0x03) << 4) | (b1 >> 4))],
        ));
        out.push(char::from(
            B64URL_ALPHA[usize::from(((b1 & 0x0F) << 2) | (b2 >> 6))],
        ));
        out.push(char::from(B64URL_ALPHA[usize::from(b2 & 0x3F)]));
    }
    out
}

// CRC-16/XMODEM: poly 0x1021, init 0x0000, no reflection, no final XOR.
fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    // From NEAR Intents chain-address-support docs (the canonical TON example).
    const INTENTS_EXAMPLE: &str = "EQAWzEKcdnykvXfUNouqdS62tvrp32bCxuKS6eQrS6ISgcLo";

    #[test]
    fn parses_intents_example() {
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        // First byte of the 32-byte hash should be 0x16 (the 'W' after EQA).
        assert_eq!(addr.0[0], 0x16);
    }

    #[test]
    fn roundtrip_intents_example() {
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        assert_eq!(addr.to_string(), INTENTS_EXAMPLE);
    }

    #[test]
    fn accepts_uq_nonbounceable_and_renders_as_eq() {
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        // Construct a UQ form with the same underlying hash.
        let mut raw = [0u8; 36];
        raw[0] = 0x51; // non-bounceable
        raw[2..34].copy_from_slice(&addr.0);
        let crc = crc16_xmodem(&raw[0..34]).to_be_bytes();
        raw[34] = crc[0];
        raw[35] = crc[1];
        let uq_form = encode_base64url_36(&raw);
        assert!(
            uq_form.starts_with("UQ"),
            "expected UQ prefix, got {uq_form}"
        );

        let parsed: TonAddress = uq_form.parse().unwrap();
        assert_eq!(parsed, addr);
        // Display flattens bounceability to EQ.
        assert_eq!(parsed.to_string(), INTENTS_EXAMPLE);
    }

    #[test]
    fn accepts_testnet_flag_and_round_trips_to_mainnet_display() {
        // The testnet bit (0x80) is a display hint only — on-chain only the
        // workchain + hash matter. We parse kQ/0Q transparently and normalise
        // Display output to EQ, so a testnet-tagged input round-trips to the
        // equivalent mainnet-bounceable representation.
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        let mut raw = [0u8; 36];
        raw[0] = 0x91; // kQ: testnet + bounceable
        raw[2..34].copy_from_slice(&addr.0);
        let crc = crc16_xmodem(&raw[0..34]).to_be_bytes();
        raw[34] = crc[0];
        raw[35] = crc[1];
        let testnet = encode_base64url_36(&raw);
        let parsed: TonAddress = testnet.parse().unwrap();
        assert_eq!(parsed, addr);
        assert_eq!(parsed.to_string(), INTENTS_EXAMPLE);
    }

    #[test]
    fn rejects_masterchain() {
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        let mut raw = [0u8; 36];
        raw[0] = 0x11;
        raw[1] = 0xFF; // workchain = -1
        raw[2..34].copy_from_slice(&addr.0);
        let crc = crc16_xmodem(&raw[0..34]).to_be_bytes();
        raw[34] = crc[0];
        raw[35] = crc[1];
        let master = encode_base64url_36(&raw);
        let err = master.parse::<TonAddress>().unwrap_err();
        assert!(err.contains("basechain"), "got: {err}");
    }

    #[test]
    fn rejects_bad_crc() {
        let mut corrupted = INTENTS_EXAMPLE.to_string();
        corrupted.pop();
        corrupted.push('A');
        let err = corrupted.parse::<TonAddress>().unwrap_err();
        assert!(err.contains("CRC"), "got: {err}");
    }

    #[test]
    fn rejects_wrong_length() {
        let err = "EQAW".parse::<TonAddress>().unwrap_err();
        assert!(err.contains("48"));
    }

    #[test]
    fn rejects_non_base64_chars() {
        let mut bad = INTENTS_EXAMPLE.to_string();
        bad.replace_range(0..1, "!");
        let err = bad.parse::<TonAddress>().unwrap_err();
        assert!(err.contains("base64"));
    }

    #[test]
    fn accepts_standard_base64_alphabet_on_decode() {
        let canonical = INTENTS_EXAMPLE.replace('-', "+").replace('_', "/");
        let parsed = canonical.parse::<TonAddress>();
        assert!(
            parsed.is_ok(),
            "standard base64 alphabet should decode: {parsed:?}"
        );
    }

    #[test]
    fn crc16_xmodem_known_vectors() {
        assert_eq!(crc16_xmodem(b""), 0x0000);
        assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
    }

    #[test]
    fn zero_address_round_trips() {
        let zero = TonAddress::ZERO;
        assert!(zero.is_zero());
        let s = zero.to_string();
        assert_eq!(s.len(), 48);
        let back: TonAddress = s.parse().unwrap();
        assert_eq!(back, zero);
    }

    #[test]
    fn serde_json_string_roundtrip() {
        let addr: TonAddress = INTENTS_EXAMPLE.parse().unwrap();
        let json = near_sdk::serde_json::to_string(&addr).unwrap();
        assert_eq!(json, format!("\"{INTENTS_EXAMPLE}\""));
        let back: TonAddress = near_sdk::serde_json::from_str(&json).unwrap();
        assert_eq!(back, addr);
    }
}
