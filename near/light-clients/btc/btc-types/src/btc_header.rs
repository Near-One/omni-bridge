use near_sdk::near;

use crate::hash::{double_sha256, H256};

pub type Error = crate::utils::DecodeHeaderError;

// Represents a Bitcoin/Litecoin/Dogecoin block header, which contains metadata about the block
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

pub type LightHeader = Header;

impl Header {
    /// The number of bytes that the block header contributes to the size of a block.
    // Serialized length of fields (version, prev_blockhash, merkle_root, time, bits, nonce)
    pub const SIZE: usize = 4 + 32 + 32 + 4 + 4 + 4; // 80

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

    pub fn from_block_header_vec(block_header: &[u8]) -> Result<Self, Error> {
        if block_header.len() != Self::SIZE {
            return Err(Error::InvalidLength);
        }

        let version = i32::from_le_bytes(
            block_header[0..4]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );
        let prev_block_hash =
            H256::try_from(&block_header[4..36]).map_err(|_| Error::InvalidLength)?;
        let merkle_root =
            H256::try_from(&block_header[36..68]).map_err(|_| Error::InvalidLength)?;
        let time = u32::from_le_bytes(
            block_header[68..72]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );
        let bits = u32::from_le_bytes(
            block_header[72..76]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );
        let nonce = u32::from_le_bytes(
            block_header[76..80]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );

        Ok(Self {
            version,
            prev_block_hash,
            merkle_root,
            time,
            bits,
            nonce,
        })
    }

    #[must_use]
    pub fn into_light(self) -> LightHeader {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Header;
    use serde_json::json;

    #[test]
    fn test_block_hash() {
        let block: Header = serde_json::from_value(json!({
            "version":536870912,
            "prevBlockHash":"ed544a1c2362b7d33f47e51dc573e69a66687d610bd777d8213954018a22d0f2",
            "merkleRoot":"40186039cb7fcc2d8efb7d3f5be9cad80d36ab9df81983805856608ca65dbd62",
            "time":1734025733,
            "bits":"1e03ffff",
            "nonce":1640674470
        }))
        .unwrap();

        assert_eq!(
            block.block_hash().to_string(),
            "cc802d4035f69e5c814b7bf3fa481cd5bd9e4ad4a10ad33a89c46467e4ea49e5"
        );
    }

    #[test]
    fn test_decode_header() {
        let block_header_hex = "04e0ff2f1d761d390c19df86dc01f970c0f53663171a75288c2406000000000000000000245470d64414a15c7333cae23c3fa9caa92cb4490f61a6a215660e09aa134e53f1e7b2607b5f0d1792aed66f";
        let block_header_bytes = hex::decode(block_header_hex).unwrap();
        let header_from_hex = Header::from_block_header_vec(&block_header_bytes).unwrap();

        let header_from_json: Header = serde_json::from_value(json!({
            "version":805298180,
            "prevBlockHash":"00000000000000000006248c28751a176336f5c070f901dc86df190c391d761d",
            "merkleRoot":"534e13aa090e6615a2a6610f49b42ca9caa93f3ce2ca33735ca11444d6705424",
            "time":1622337521,
            "bits":"170d5f7b",
            "nonce":1876340370
        }))
        .unwrap();

        assert_eq!(header_from_hex, header_from_json);
        assert_eq!(
            header_from_hex.block_hash().to_string(),
            "000000000000000000016f0484972d135afba541c837d0c07c1530ffeee293cd"
        );
    }
}
