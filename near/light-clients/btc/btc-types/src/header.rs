use near_sdk::near;

use crate::{
    hash::{double_sha256, H256},
    u256::U256,
};

pub type Target = U256;
pub type Work = U256;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Block version, now repurposed for soft fork signalling.
    pub version: i32,
    /// Reference to the previous block in the chain.
    pub prev_block_hash: H256,
    /// The root hash of the merkle tree of transactions in the block.
    pub merkle_root: H256,
    /// The timestamp of the block, as claimed by the miner.
    pub time: u32,
    /// The target value below which the blockhash must lie.
    pub bits: u32,
    /// The nonce, selected to obtain a low enough blockhash.
    pub nonce: u32,
}

impl Header {
    /// The number of bytes that the block header contributes to the size of a block.
    // Serialized length of fields (version, prev_blockhash, merkle_root, time, bits, nonce)
    pub const SIZE: usize = 4 + 32 + 32 + 4 + 4 + 4; // 80

    /// Computes the target (range [0, T] inclusive) that a blockhash must land in to be valid.
    #[must_use]
    pub fn target(&self) -> Target {
        // This is a floating-point "compact" encoding originally used by
        // OpenSSL, which satoshi put into consensus code, so we're stuck
        // with it. The exponent needs to have 3 subtracted from it, hence
        // this goofy decoding code. 3 is due to 3 bytes in the mantissa.
        let (mant, expt) = {
            let unshifted_expt = self.bits >> 24;
            if unshifted_expt <= 3 {
                ((self.bits & 0x00FF_FFFF) >> (8 * (3 - unshifted_expt)), 0)
            } else {
                (self.bits & 0x00FF_FFFF, 8 * (unshifted_expt - 3))
            }
        };

        // The mantissa is signed but may not be negative.
        if mant > 0x7F_FFFF {
            Target::ZERO
        } else {
            U256::from(mant) << expt
        }
    }

    /// Returns the total work of the block.
    /// "Work" is defined as the work done to mine a block with this target value (recorded in the
    /// block header in compact form as nBits).
    #[must_use]
    pub fn work(&self) -> Work {
        self.target().inverse()
    }

    #[must_use]
    pub fn block_hash(&self) -> H256 {
        let block_header = self.get_block_header_vec();
        double_sha256(&block_header)
    }

    pub fn block_hash_pow(&self) -> H256 {
        let block_header = self.get_block_header_vec();
        #[cfg(feature = "scrypt_hash")]
        {
            let params = scrypt::Params::new(10, 1, 1, 32).unwrap(); // N=1024 (2^10), r=1, p=1

            let mut output = [0u8; 32];
            scrypt::scrypt(&block_header, &block_header, &params, &mut output).unwrap();
            H256::from(output)
        }

        #[cfg(not(feature = "scrypt_hash"))]
        {
            double_sha256(&block_header)
        }
    }

    fn get_block_header_vec(&self) -> Vec<u8> {
        let mut block_header = Vec::with_capacity(Self::SIZE);
        block_header.extend_from_slice(&self.version.to_le_bytes());
        block_header.extend(self.prev_block_hash.0);
        block_header.extend(self.merkle_root.0);
        block_header.extend_from_slice(&self.time.to_le_bytes());
        block_header.extend_from_slice(&self.bits.to_le_bytes());
        block_header.extend_from_slice(&self.nonce.to_le_bytes());

        block_header
    }
}

#[allow(clippy::module_name_repetitions)]
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedHeader {
    pub block_header: Header,
    /// Below, state contains additional fields not presented in the standard blockchain header
    /// those fields are used to represent additional information required for fork management
    /// and other utility functionality
    ///
    /// Current `block_hash`
    pub block_hash: H256,
    /// Accumulated chainwork at this position for this block
    pub chain_work: Work,
    /// Block height in the Bitcoin network
    pub block_height: u64,
}

#[cfg(test)]
mod tests {
    use super::Header;
    use serde_json::json;

    #[test]
    fn test_block_hash() {
        let block: Header = serde_json::from_value(json!({
            "version":536870912,
            "prev_block_hash":"ed544a1c2362b7d33f47e51dc573e69a66687d610bd777d8213954018a22d0f2",
            "merkle_root":"40186039cb7fcc2d8efb7d3f5be9cad80d36ab9df81983805856608ca65dbd62",
            "time":1734025733,
            "bits":503578623,
            "nonce":1640674470
        }))
        .unwrap();

        assert_eq!(
            block.block_hash().to_string(),
            "cc802d4035f69e5c814b7bf3fa481cd5bd9e4ad4a10ad33a89c46467e4ea49e5"
        );
    }
}
