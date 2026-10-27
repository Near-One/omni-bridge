pub use ethereum_types::{Address, Bloom, H256, H64, U256, U64};
use rlp::{Decodable, DecoderError, Encodable, Rlp, RlpStream};

use crate::utils::keccak256;

#[allow(clippy::module_name_repetitions)]
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
    pub requests_hash: Option<H256>,
    // Amsterdam: EIP-7928 (block-level access lists) and EIP-7843 (SLOTNUM)
    pub block_access_list_hash: Option<H256>,
    pub slot_number: Option<U64>,

    pub hash: Option<H256>,
}

struct CustomRlpIter<'a> {
    index: usize,
    consumed_bytes: usize,
    rlp: &'a Rlp<'a>,
}

impl<'a> CustomRlpIter<'a> {
    const fn new(rlp: &'a Rlp) -> Self {
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
                Ok(Some(result.as_val()?))
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
        let mut block_header = Self {
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
            requests_hash: iter.next_option()?,
            block_access_list_hash: iter.next_option()?,
            slot_number: iter.next_option()?,
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
        self.requests_hash.as_ref().map(|v| stream.append(v));
        self.block_access_list_hash
            .as_ref()
            .map(|v| stream.append(v));
        self.slot_number.map(|v| stream.append(&v));

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
        let _header: BlockHeader = rlp::decode(&header_rlp).unwrap();
    }

    #[test]
    fn decode_header_hex() {
        let header_rlp = hex::decode("f9021fa03de6bb3849a138e6ab0b83a3a00dc7433f1e83f7fd488e4bba78f2fe2631a633a01dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347947777788200b672a42421017f65ede4fc759564c8a041cf6e8e60fd087d2b00360dc29e5bfb21959bce1f4c242fd1ad7c4da968eb87a0dfcb68d3a3c41096f4a77569db7956e0a0e750fad185948e54789ea0e51779cba08a8865cd785e2e9dfce7da83aca010b10b9af2abbd367114b236f149534c821db9010024e74ad77d9a2b27bdb8f6d6f7f1cffdd8cfb47fdebd433f011f7dfcfbb7db638fadd5ff66ed134ede2879ce61149797fbcdf7b74f6b7de153ec61bdaffeeb7b59c3ed771a2fe9eaed8ac70e335e63ff2bfe239eaff8f94ca642fdf7ee5537965be99a440f53d2ce057dbf9932be9a7b9a82ffdffe4eeee1a66c4cfb99fe4540fbff936f97dde9f6bfd9f8cefda2fc174d23dfdb7d6f7dfef5f754fe6a7eec92efdbff779b5feff3beafebd7fd6e973afebe4f5d86f3aafb1f73bf1e1d0cdd796d89827edeffe8fb6ae6d7bf639ec5f5ff4c32f31f6b525b676c7cdf5e5c75bfd5b7bd1928b6f43aac7fa0f6336576e5f7b7dfb9e8ebbe6f6efe2f9dfe8b3f56871b81c1fe05b21883c5d4888401ca35428401ca262984610bdaa69768747470733a2f2f7777772e6b7279707465782e6f7267a09620b46a81a4795cf4449d48e3270419f58b09293a5421205f88179b563f815a88b223da049adf2216843b9aca00").unwrap();
        let _header: BlockHeader = rlp::decode(&header_rlp).unwrap();
    }

    /// Sepolia block 11926951, which contains tx
    /// `0x602717b8b7ebd2e6b201c7dab3df07252f732f59d641418d96970406dccf29cb`.
    /// Post-Amsterdam headers carry two extra RLP fields, `blockAccessListHash`
    /// (EIP-7928) and `slotNumber` (EIP-7843); before they were decoded the trailing
    /// bytes made `decode` fail with `RlpInconsistentLengthAndData`.
    #[test]
    fn decode_post_amsterdam_header() {
        let header_rlp = hex::decode("f9029fa0a89d3d1b12c9199fd122a781c4ef3d0cb249ecae862d918a4f69cf2edcaebbb5a01dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347948f614cdb61e37b6f18d9705942c93fa86b04d711a03a70bf84c4ef2b80331addd04fdbfd8577e4f5e75f26f1dccb0975e0cd4b6567a077c897bb14435651b7bf513bbe61dde5affcd6229823354c4ac9279534f24e81a044b9740a8ace578fbf98a3fcd5cc4c1bc6b9a5ee6c30befa4505d415413ded28b90100d4ba40ecd034c3774af2419fa5a4020f193652fd8989c23f28d0d11667aba7532d9d6d235d70629cb07805de583561eca119982c56b784111e271cf9ecef75441b03bec54b3e1542f9e965dfc2f21030865f96ff35c4369a5caa86ee8e9136707ec61c822bbbfcd271829c6220b8cd652aaf1efaa42c5ce98610d3fb494022cfc3ce19e31554a99ca9fbc95c07d0ddf5f1603e8f694295a939d71d42031c36603a897e571c1c258e4f4147c1e8bbb16f84ee3ce98a944a6efce048a20d084ae96a7d090235abe70a9d2e7c2b2bb39892fad0c3c0844e917472133d5bc1c3efd9a0da131bc1e66803a84b19416849eef4b2a67d8afda00b49bbdd0bc7685f4a1d8083b5fda7840bebc2008405a90745846ad2053c957a6275696c64657220313639396438343631323064a0274676613ebb30970becb091814972ea8b728ee06b933df502854911c0fbebf98800000000000000000fa04e69ef5a47dfd730b2539ebd5dd99a99bb306848c3555c49739978aa554753bf8314000080a09d94c307b05fb1ba37cd623c49e2a9e6722a4e1ec67a25198998275854a43133a0e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855a0c3e5d511b1fa52edf67bc8624d4a58461625fb1dfc39744af391c9c8bee28aab83ad75fd").unwrap();
        let header: BlockHeader = rlp::decode(&header_rlp).unwrap();

        assert_eq!(header.number, U64::from(11_926_951));
        assert_eq!(
            header.block_access_list_hash,
            Some(
                "0xc3e5d511b1fa52edf67bc8624d4a58461625fb1dfc39744af391c9c8bee28aab"
                    .parse()
                    .unwrap()
            )
        );
        assert_eq!(header.slot_number, Some(U64::from(0x00ad_75fd)));

        // The light client matches on this hash, so it must be the real block hash.
        assert_eq!(
            header.hash,
            Some(
                "0x44b5721009eeee62e1d2c805f51f3df11deba052a6a27cb03a16d47f406d43b9"
                    .parse()
                    .unwrap()
            )
        );
        // Re-encoding must be byte-identical, otherwise the hash above is not reproducible.
        assert_eq!(rlp::encode(&header).to_vec(), header_rlp);
    }
}
