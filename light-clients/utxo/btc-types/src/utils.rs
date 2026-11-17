use crate::u256::U256;

pub mod serd_u32_hex {
    pub fn serialize<S>(num: &u32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let hex = hex::encode(num.to_le_bytes()).to_string();
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_str: String = serde::Deserialize::deserialize(deserializer)?;
        u32::from_str_radix(&hex_str, 16).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub enum DecodeHeaderError {
    InvalidLength,
    IntParseError,
}

impl std::fmt::Display for DecodeHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeHeaderError::InvalidLength => write!(f, "Invalid length"),
            DecodeHeaderError::IntParseError => write!(f, "Integer parse error"),
        }
    }
}

impl std::error::Error for DecodeHeaderError {}

/// Computes the target (range [0, T] inclusive) that a blockhash must land in to be valid.
#[must_use]
pub fn target_from_bits(bits: u32) -> U256 {
    let (mant, expt) = {
        let unshifted_expt = bits >> 24;
        if unshifted_expt <= 3 {
            ((bits & 0x00FF_FFFF) >> (8 * (3 - unshifted_expt)), 0)
        } else {
            (bits & 0x00FF_FFFF, 8 * (unshifted_expt - 3))
        }
    };

    if mant > 0x7F_FFFF {
        U256::ZERO
    } else {
        U256::from(mant) << expt
    }
}

/// Returns the total work of the block.
#[must_use]
pub fn work_from_bits(bits: u32) -> U256 {
    target_from_bits(bits).inverse()
}
