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
/// Returns zero for negative or overflowing `bits`.
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

    if mant > 0x7F_FFFF || (mant != 0 && (32 - mant.leading_zeros()) + expt > 256) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_from_bits_overflow() {
        assert_eq!(target_from_bits(0x2200_00ff), U256::from(0xffu8) << 248);
        assert_eq!(target_from_bits(0x2100_ffff), U256::from(0xffffu16) << 240);
        for bits in [
            0x2200_0100,
            0x2201_ffff,
            0x2300_0001,
            0x2400_ffff,
            0xff00_ffff,
        ] {
            assert_eq!(target_from_bits(bits), U256::ZERO, "bits {bits:#x}");
        }
    }
}
