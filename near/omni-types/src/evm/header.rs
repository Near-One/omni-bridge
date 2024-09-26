pub use ethereum_types::{Address, Bloom, H256, H64, U256, U64};
use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};

use super::utils::keccak256;

#[derive(Default, Debug, Clone)]
pub struct BlockHeader {
    pub parent_hash: H256,
    pub sha3_uncles: H256,
    pub miner: Address,
    pub state_root: H256,
    pub transactions_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub difficulty: U256,
    pub number: U64,
    pub gas_limit: U256,
    pub gas_used: U256,
    pub timestamp: U64,
    pub extra_data: Vec<u8>,
    pub mix_hash: H256,
    pub nonce: H64,
    pub base_fee_per_gas: Option<U64>,
    pub withdrawals_root: Option<H256>,
    pub blob_gas_used: Option<U64>,
    pub excess_blob_gas: Option<U64>,
    pub parent_beacon_block_root: Option<H256>,
    pub hash: Option<H256>,
}

struct CustomRlpIter<'a> {
    index: usize,
    consumed_bytes: usize,
    rlp: &'a Rlp<'a>,
}

impl<'a> CustomRlpIter<'a> {
    fn new(rlp: &'a Rlp) -> Self {
        Self {
            index: 0,
            consumed_bytes: 0,
            rlp,
        }
    }

    fn next<T: Decodable>(&mut self) -> Result<T, DecoderError> {
        let result = self.rlp.at(self.index)?;
        self.consumed_bytes += result.as_raw().len();
        self.index += 1;
        result.as_val()
    }

    fn next_option<T: Decodable>(&mut self) -> Result<Option<T>, DecoderError> {
        match self.rlp.at(self.index) {
            Ok(result) => {
                self.consumed_bytes += result.as_raw().len();
                self.index += 1;
                result.as_val()
            }
            Err(_) => Ok(None),
        }
    }

    fn is_all_bytes_consumed(&self) -> Result<bool, DecoderError> {
        Ok(self.rlp.as_raw().len() == self.rlp.payload_info()?.header_len + self.consumed_bytes)
    }
}

impl Decodable for BlockHeader {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let mut iter = CustomRlpIter::new(rlp);
        let mut block_header = BlockHeader {
            parent_hash: iter.next()?,
            sha3_uncles: iter.next()?,
            miner: iter.next()?,
            state_root: iter.next()?,
            transactions_root: iter.next()?,
            receipts_root: iter.next()?,
            logs_bloom: iter.next()?,
            difficulty: iter.next()?,
            number: iter.next()?,
            gas_limit: iter.next()?,
            gas_used: iter.next()?,
            timestamp: iter.next()?,
            extra_data: iter.next()?,
            mix_hash: iter.next()?,
            nonce: iter.next()?,
            base_fee_per_gas: iter.next_option()?,
            withdrawals_root: iter.next_option()?,
            blob_gas_used: iter.next_option()?,
            excess_blob_gas: iter.next_option()?,
            parent_beacon_block_root: iter.next_option()?,
            hash: None,
        };

        if !iter.is_all_bytes_consumed()? {
            return Err(DecoderError::RlpInconsistentLengthAndData);
        }

        block_header.hash = Some(keccak256(rlp.as_raw()).into());

        Ok(block_header)
    }
}

impl Encodable for BlockHeader {
    fn rlp_append(&self, stream: &mut RlpStream) {
        stream.begin_unbounded_list();
        stream
            .append(&self.parent_hash)
            .append(&self.sha3_uncles)
            .append(&self.miner)
            .append(&self.state_root)
            .append(&self.transactions_root)
            .append(&self.receipts_root)
            .append(&self.logs_bloom)
            .append(&self.difficulty)
            .append(&self.number)
            .append(&self.gas_limit)
            .append(&self.gas_used)
            .append(&self.timestamp)
            .append(&self.extra_data)
            .append(&self.mix_hash)
            .append(&self.nonce);

        self.base_fee_per_gas.map(|v| stream.append(&v));
        self.withdrawals_root.as_ref().map(|v| stream.append(v));
        self.blob_gas_used.map(|v| stream.append(&v));
        self.excess_blob_gas.map(|v| stream.append(&v));
        self.parent_beacon_block_root
            .as_ref()
            .map(|v| stream.append(v));

        stream.finalize_unbounded_list();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_header_with_extra_bytes() {
        let mut header_rlp = rlp::encode(&BlockHeader::default()).to_vec();
        header_rlp.push(180);
        let header: Result<BlockHeader, DecoderError> = rlp::decode(&header_rlp);
        assert_eq!(
            header.unwrap_err(),
            DecoderError::RlpInconsistentLengthAndData
        );
    }

    #[test]
    fn decode_header() {
        let header_rlp = rlp::encode(&BlockHeader::default()).to_vec();
        let header: Result<BlockHeader, DecoderError> = rlp::decode(&header_rlp);
        assert!(header.is_ok())
    }
}
