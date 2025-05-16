use ethereum_types::{Address, Bloom, H256, U256};
use rlp::{Decodable, DecoderError, Encodable, Rlp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub status: bool,
    pub gas_used: U256,
    pub log_bloom: Bloom,
    pub logs: Vec<LogEntry>,
}

impl Decodable for Receipt {
    fn decode(rlp: &Rlp) -> Result<Self, DecoderError> {
        let mut view = rlp.as_raw();

        // https://eips.ethereum.org/EIPS/eip-2718
        if let Some(&byte) = view.first() {
            // https://eips.ethereum.org/EIPS/eip-2718#receipts
            // If the first byte is between 0 and 0x7f it is an envelop receipt
            if byte <= 0x7f {
                view = &view[1..];
            }
        }

        let rlp = Rlp::new(view);
        Ok(Self {
            status: rlp.val_at(0)?,
            gas_used: rlp.val_at(1)?,
            log_bloom: rlp.val_at(2)?,
            logs: rlp.list_at(3)?,
        })
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

impl Decodable for LogEntry {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let result = Self {
            address: rlp.val_at(0)?,
            topics: rlp.list_at(1)?,
            data: rlp.val_at(2)?,
        };
        Ok(result)
    }
}

impl Encodable for LogEntry {
    fn rlp_append(&self, stream: &mut rlp::RlpStream) {
        stream.begin_list(3);
        stream.append(&self.address);
        stream.append_list::<H256, _>(&self.topics);
        stream.append(&self.data);
    }
}
