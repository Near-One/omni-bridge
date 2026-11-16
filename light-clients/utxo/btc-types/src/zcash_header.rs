use near_sdk::near;

use crate::hash::{double_sha256, H256};

pub type Error = crate::utils::DecodeHeaderError;

// Represents a Zcash block header, which contains metadata about the block
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Block version, now repurposed for soft fork signalling.
    pub version: i32,
    /// Reference to the previous block in the chain.
    pub prev_block_hash: H256,
    /// The root hash of the merkle tree of transactions in the block.
    pub merkle_root: H256,
    /// The root hash of the block commitments tree.
    pub block_commitments: H256,
    /// The timestamp of the block, as claimed by the miner.
    pub time: u32,
    /// The target value below which the blockhash must lie.
    pub bits: u32,
    // The block's nonce (Zcash: 32 bytes)
    pub nonce: H256,
    /// The block solution (Zcash: Equihash solution)
    #[serde(deserialize_with = "hex::serde::deserialize")]
    #[serde(serialize_with = "hex::serde::serialize")]
    pub solution: Vec<u8>,
}

impl Header {
    /// The number of bytes that the block header contributes to the size of a block.
    // Serialized length of fields (version, prev_blockhash, merkle_root, time, bits, nonce, solution)
    pub const SIZE: usize = 4 + 32 + 32 + 32 + 4 + 4 + 32 + 3 + 1344; // 1400
    pub const SIZE_FOR_EQUIHASH: usize = 4 + 32 + 32 + 32 + 4 + 4; // 108 excluding nonce and Equihash solution

    #[must_use]
    pub fn block_hash(&self) -> H256 {
        let block_header = self.get_block_header_vec();
        double_sha256(&block_header)
    }

    pub fn block_hash_pow(&self) -> H256 {
        let block_header = self.get_block_header_vec();
        double_sha256(&block_header)
    }

    fn get_block_header_vec(&self) -> Vec<u8> {
        let mut block_header = Vec::with_capacity(Self::SIZE);
        block_header.extend_from_slice(&self.version.to_le_bytes());
        block_header.extend(self.prev_block_hash.0);
        block_header.extend(self.merkle_root.0);
        block_header.extend(self.block_commitments.0);
        block_header.extend_from_slice(&self.time.to_le_bytes());
        block_header.extend_from_slice(&self.bits.to_le_bytes());
        block_header.extend_from_slice(&self.nonce.0);
        block_header.extend_from_slice(&[0xfd, 0x40, 0x05]); // The compact size of an Equihash solution in bytes (always 1344).
        block_header.extend_from_slice(&self.solution);

        block_header
    }

    // The block header minus nonce and solution.
    pub fn get_block_header_vec_for_equihash(&self) -> Vec<u8> {
        let mut block_header = Vec::with_capacity(Self::SIZE_FOR_EQUIHASH);
        block_header.extend_from_slice(&self.version.to_le_bytes());
        block_header.extend(self.prev_block_hash.0);
        block_header.extend(self.merkle_root.0);
        block_header.extend(self.block_commitments.0);
        block_header.extend_from_slice(&self.time.to_le_bytes());
        block_header.extend_from_slice(&self.bits.to_le_bytes());

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

        let block_commitments =
            H256::try_from(&block_header[68..100]).map_err(|_| Error::InvalidLength)?;
        let time = u32::from_le_bytes(
            block_header[100..104]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );
        let bits = u32::from_le_bytes(
            block_header[104..108]
                .try_into()
                .map_err(|_| Error::IntParseError)?,
        );
        let nonce = H256::try_from(&block_header[108..140]).map_err(|_| Error::InvalidLength)?;
        let solution = block_header[143..].to_vec();

        Ok(Self {
            version,
            prev_block_hash,
            merkle_root,
            block_commitments,
            time,
            bits,
            nonce,
            solution,
        })
    }

    pub fn into_light(self) -> LightHeader {
        self.into()
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
// The header, excluding nonce and Equihash solution
pub struct LightHeader {
    pub version: i32,
    pub prev_block_hash: H256,
    pub merkle_root: H256,
    pub block_commitments: H256,
    pub time: u32,
    pub bits: u32,
}

impl From<Header> for LightHeader {
    fn from(header: Header) -> Self {
        Self {
            version: header.version,
            prev_block_hash: header.prev_block_hash,
            merkle_root: header.merkle_root,
            block_commitments: header.block_commitments,
            time: header.time,
            bits: header.bits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Header;
    use serde_json::json;

    #[test]
    fn test_block_hash() {
        let block: Header = serde_json::from_value(json!({
            "version":4,
            "prevBlockHash":"00000000010e4f01fd87fa5f72960873172409fff28827d5f54e0b216fc61cc2",
            "merkleRoot":"048a18d050c88aa5813f6b34873c59541d78aa516b373edf7d64cd3625ab245a",
            "blockCommitments": "1f6f4ed77ce5375f1fd7ecdf2a742f53c047ef315af971ad8c80477a2872ba5d",
            "time":1747234339,
            "bits":"1c01c0cb",
            "nonce":"cf6507400000000000000000006b0000000000000000000000000000a2c9a99d",
            "solution":"0015bbee89f64895dccfd1f4ada6c629480a5577e00870ef5699864ad8f7572351bc2421c14686bc12de02e803dc96015af6556d67c3d7e1839a55a55cd4ed0b1a9aff866a37abb963d50387b15fa5fe90d65fbd0e5ce342c6c6997875cc211961bec11ca2debe5131106172aee3dc67579119c67d57d1691e5613f36f2b269f1f8b6511f7f2f50093bd98c7b6022dad5b36e951721d83201b40ad691ceaa57359baa2f08898f60c02ffb9d956022f8ebae0a07bfa3bd34e2c0a581dcc06722347f41a6b13860ec6f4c43a6f19fc61f3a1710fd6cf21e58d94a73f9a718ad79ff9163928b36fea2386124e9d98a3f37f585504e6e6204649b47c29e507af489dddd1030ef3ef11a5d679be291f2b56045d095a2a1bd991480d6caca57f1ddbcace2515158cc30b5e84b284cf32c38042a45d22349826ad9b1b64a527acbdc9b69a94a4e12e84c970b48c8d52259b074604bc77889ea912279c67f3d6d7a561aa44f6724ec443cfc2f2ce940649a33ed47fe8b374b5378f102bf5102391c58fd2dbfaa88ec7953a4a9c15efb21ddd9b32106d599116de5561d627efc87d518b5f1edc32f0057d9d820c4cb3bd9ce573b6a4ebf9cd955c530e6d2b7b050c76283433f7d434509fc5334366711e15790eb9ab4cbc27e0d94b64b4c5fa6f0d3170dd4f594f17d92f4b0672e265eedb06b54e4408edb4fd168cac051c34ed925d307bb420420a6db1d1f7dfa2df01e1617eb77adda1f271882767c917cf0ec3246b5ab0351cb62dc6055bd0b3aba7b3abd646f8770717d99c7d2ae473c18920f7c3c20987308ad2a63ec3a2f7e57f053e40754211386113e551a100a4cc5dfa0d9394a407b8c46409465b74f21006e88de9bf37b44e1f80600d4256c6a5232a3f415f6271d3eed7ce7da238ee2341e207f3a315e494b771a4b809a64fadc92f99a55a00e65ce0f094471119202017273991e8e9a154b06c3ad04e2764d892a9f29284da23f5e49d61968f0ac814d1bbf4a7d7394cfbc4953125eb7b12631f750745351639a9225cc7ef7281e9046d60218edbb31aefc208f4538a8354e0c1296f82e2fccd78a223a11398cd11d4533fba9e99e9f766a5d04ad2beae8d3b39cb5d0aad387fa26201bb456af1a15ff1bc38dac375a796344e4d6cfc4fe7f29c65586e8b4e268b1fe6befcdb0153ad7538736743a67aa8ee3473be5adb371b9b04190a0dd91aec6fc5ed4e12c715408ee512529ede8b2518c63351df6449baed226e6b2a2761cba43ec7044ade9e5a51e63a15648a15a276ec000ef7d1bd4c610d05a39355487ca9f3a394051ffd82719a1badf4a90fef09a6a76d2efbc044947b1fd38d95505bf1dac123f39ae4b090dcb53ce4e50086f1e77f347abc519a287db98b321d68557770d3cef520b73a9df99d736b043dcf4ef7c586ad626d94358d4252f9b17f78c7a51fb4ce2fa5f1c67db045f26ee54b36c555e02fe3b826331517964f67330d5fb5dcddbee24de9c490e5c239ed675bd39845954c28b42c374393370a115ea7a91bdcfa4775884c1ce7d2f305a36a97ce3155f8dd7c1f0a6c9044abd489b50744beb4df58e6a44d97b20c222cdfc3e4d42d4346c1b72befcd9305dfa95f1c16346eaf08e923d9ef618184184fc7dcf1645a3602f00a4ba2a9ab115ec97bc614fe9d395baddbe9d4d86957a6bebe31761901da49e5eb2155f82594aaf3ffd70ba42e278d84d11b1ace451b826fd639535b7a5a30152684e212e655f37de774ac6eb89732611bbc751f0f49fa94c694cd6faa13b83b07db8be6f59e5d093413fa038df98963fa84ff6195d0c850e0c7993db95c129cc614984998a941fee834aed814128838f5e76924b14e5409f3a565b53a66093ec676561ccb56b8ec",
        }))
        .unwrap();

        assert_eq!(
            block.block_hash().to_string(),
            "00000000012860b68fc02b728ef20b64c2b15714c988d0664c54e0ace815b1bd"
        );
    }

    #[test]
    fn test_decode_header() {
        let block_header_hex = "04000000c21cc66f210b4ef5d52788f2ff092417730896725ffa87fd014f0e01000000005a24ab2536cd647ddf3e376b51aa781d54593c87346b3f81a58ac850d0188a045dba72287a47808cad71f95a31ef47c0532f742adfecd71f5f37e57cd74e6f1f23ae2468cbc0011c9da9c9a200000000000000000000000000006b000000000000000000400765cffd40050015bbee89f64895dccfd1f4ada6c629480a5577e00870ef5699864ad8f7572351bc2421c14686bc12de02e803dc96015af6556d67c3d7e1839a55a55cd4ed0b1a9aff866a37abb963d50387b15fa5fe90d65fbd0e5ce342c6c6997875cc211961bec11ca2debe5131106172aee3dc67579119c67d57d1691e5613f36f2b269f1f8b6511f7f2f50093bd98c7b6022dad5b36e951721d83201b40ad691ceaa57359baa2f08898f60c02ffb9d956022f8ebae0a07bfa3bd34e2c0a581dcc06722347f41a6b13860ec6f4c43a6f19fc61f3a1710fd6cf21e58d94a73f9a718ad79ff9163928b36fea2386124e9d98a3f37f585504e6e6204649b47c29e507af489dddd1030ef3ef11a5d679be291f2b56045d095a2a1bd991480d6caca57f1ddbcace2515158cc30b5e84b284cf32c38042a45d22349826ad9b1b64a527acbdc9b69a94a4e12e84c970b48c8d52259b074604bc77889ea912279c67f3d6d7a561aa44f6724ec443cfc2f2ce940649a33ed47fe8b374b5378f102bf5102391c58fd2dbfaa88ec7953a4a9c15efb21ddd9b32106d599116de5561d627efc87d518b5f1edc32f0057d9d820c4cb3bd9ce573b6a4ebf9cd955c530e6d2b7b050c76283433f7d434509fc5334366711e15790eb9ab4cbc27e0d94b64b4c5fa6f0d3170dd4f594f17d92f4b0672e265eedb06b54e4408edb4fd168cac051c34ed925d307bb420420a6db1d1f7dfa2df01e1617eb77adda1f271882767c917cf0ec3246b5ab0351cb62dc6055bd0b3aba7b3abd646f8770717d99c7d2ae473c18920f7c3c20987308ad2a63ec3a2f7e57f053e40754211386113e551a100a4cc5dfa0d9394a407b8c46409465b74f21006e88de9bf37b44e1f80600d4256c6a5232a3f415f6271d3eed7ce7da238ee2341e207f3a315e494b771a4b809a64fadc92f99a55a00e65ce0f094471119202017273991e8e9a154b06c3ad04e2764d892a9f29284da23f5e49d61968f0ac814d1bbf4a7d7394cfbc4953125eb7b12631f750745351639a9225cc7ef7281e9046d60218edbb31aefc208f4538a8354e0c1296f82e2fccd78a223a11398cd11d4533fba9e99e9f766a5d04ad2beae8d3b39cb5d0aad387fa26201bb456af1a15ff1bc38dac375a796344e4d6cfc4fe7f29c65586e8b4e268b1fe6befcdb0153ad7538736743a67aa8ee3473be5adb371b9b04190a0dd91aec6fc5ed4e12c715408ee512529ede8b2518c63351df6449baed226e6b2a2761cba43ec7044ade9e5a51e63a15648a15a276ec000ef7d1bd4c610d05a39355487ca9f3a394051ffd82719a1badf4a90fef09a6a76d2efbc044947b1fd38d95505bf1dac123f39ae4b090dcb53ce4e50086f1e77f347abc519a287db98b321d68557770d3cef520b73a9df99d736b043dcf4ef7c586ad626d94358d4252f9b17f78c7a51fb4ce2fa5f1c67db045f26ee54b36c555e02fe3b826331517964f67330d5fb5dcddbee24de9c490e5c239ed675bd39845954c28b42c374393370a115ea7a91bdcfa4775884c1ce7d2f305a36a97ce3155f8dd7c1f0a6c9044abd489b50744beb4df58e6a44d97b20c222cdfc3e4d42d4346c1b72befcd9305dfa95f1c16346eaf08e923d9ef618184184fc7dcf1645a3602f00a4ba2a9ab115ec97bc614fe9d395baddbe9d4d86957a6bebe31761901da49e5eb2155f82594aaf3ffd70ba42e278d84d11b1ace451b826fd639535b7a5a30152684e212e655f37de774ac6eb89732611bbc751f0f49fa94c694cd6faa13b83b07db8be6f59e5d093413fa038df98963fa84ff6195d0c850e0c7993db95c129cc614984998a941fee834aed814128838f5e76924b14e5409f3a565b53a66093ec676561ccb56b8ec";
        let block_header_bytes = hex::decode(block_header_hex).unwrap();
        let header_from_hex = Header::from_block_header_vec(&block_header_bytes).unwrap();

        let header_from_json: Header = serde_json::from_value(json!({
            "version":4,
            "prevBlockHash":"00000000010e4f01fd87fa5f72960873172409fff28827d5f54e0b216fc61cc2",
            "merkleRoot":"048a18d050c88aa5813f6b34873c59541d78aa516b373edf7d64cd3625ab245a",
            "blockCommitments": "1f6f4ed77ce5375f1fd7ecdf2a742f53c047ef315af971ad8c80477a2872ba5d",
            "time":1747234339,
            "bits":"1c01c0cb",
            "nonce":"cf6507400000000000000000006b0000000000000000000000000000a2c9a99d",
            "solution":"0015bbee89f64895dccfd1f4ada6c629480a5577e00870ef5699864ad8f7572351bc2421c14686bc12de02e803dc96015af6556d67c3d7e1839a55a55cd4ed0b1a9aff866a37abb963d50387b15fa5fe90d65fbd0e5ce342c6c6997875cc211961bec11ca2debe5131106172aee3dc67579119c67d57d1691e5613f36f2b269f1f8b6511f7f2f50093bd98c7b6022dad5b36e951721d83201b40ad691ceaa57359baa2f08898f60c02ffb9d956022f8ebae0a07bfa3bd34e2c0a581dcc06722347f41a6b13860ec6f4c43a6f19fc61f3a1710fd6cf21e58d94a73f9a718ad79ff9163928b36fea2386124e9d98a3f37f585504e6e6204649b47c29e507af489dddd1030ef3ef11a5d679be291f2b56045d095a2a1bd991480d6caca57f1ddbcace2515158cc30b5e84b284cf32c38042a45d22349826ad9b1b64a527acbdc9b69a94a4e12e84c970b48c8d52259b074604bc77889ea912279c67f3d6d7a561aa44f6724ec443cfc2f2ce940649a33ed47fe8b374b5378f102bf5102391c58fd2dbfaa88ec7953a4a9c15efb21ddd9b32106d599116de5561d627efc87d518b5f1edc32f0057d9d820c4cb3bd9ce573b6a4ebf9cd955c530e6d2b7b050c76283433f7d434509fc5334366711e15790eb9ab4cbc27e0d94b64b4c5fa6f0d3170dd4f594f17d92f4b0672e265eedb06b54e4408edb4fd168cac051c34ed925d307bb420420a6db1d1f7dfa2df01e1617eb77adda1f271882767c917cf0ec3246b5ab0351cb62dc6055bd0b3aba7b3abd646f8770717d99c7d2ae473c18920f7c3c20987308ad2a63ec3a2f7e57f053e40754211386113e551a100a4cc5dfa0d9394a407b8c46409465b74f21006e88de9bf37b44e1f80600d4256c6a5232a3f415f6271d3eed7ce7da238ee2341e207f3a315e494b771a4b809a64fadc92f99a55a00e65ce0f094471119202017273991e8e9a154b06c3ad04e2764d892a9f29284da23f5e49d61968f0ac814d1bbf4a7d7394cfbc4953125eb7b12631f750745351639a9225cc7ef7281e9046d60218edbb31aefc208f4538a8354e0c1296f82e2fccd78a223a11398cd11d4533fba9e99e9f766a5d04ad2beae8d3b39cb5d0aad387fa26201bb456af1a15ff1bc38dac375a796344e4d6cfc4fe7f29c65586e8b4e268b1fe6befcdb0153ad7538736743a67aa8ee3473be5adb371b9b04190a0dd91aec6fc5ed4e12c715408ee512529ede8b2518c63351df6449baed226e6b2a2761cba43ec7044ade9e5a51e63a15648a15a276ec000ef7d1bd4c610d05a39355487ca9f3a394051ffd82719a1badf4a90fef09a6a76d2efbc044947b1fd38d95505bf1dac123f39ae4b090dcb53ce4e50086f1e77f347abc519a287db98b321d68557770d3cef520b73a9df99d736b043dcf4ef7c586ad626d94358d4252f9b17f78c7a51fb4ce2fa5f1c67db045f26ee54b36c555e02fe3b826331517964f67330d5fb5dcddbee24de9c490e5c239ed675bd39845954c28b42c374393370a115ea7a91bdcfa4775884c1ce7d2f305a36a97ce3155f8dd7c1f0a6c9044abd489b50744beb4df58e6a44d97b20c222cdfc3e4d42d4346c1b72befcd9305dfa95f1c16346eaf08e923d9ef618184184fc7dcf1645a3602f00a4ba2a9ab115ec97bc614fe9d395baddbe9d4d86957a6bebe31761901da49e5eb2155f82594aaf3ffd70ba42e278d84d11b1ace451b826fd639535b7a5a30152684e212e655f37de774ac6eb89732611bbc751f0f49fa94c694cd6faa13b83b07db8be6f59e5d093413fa038df98963fa84ff6195d0c850e0c7993db95c129cc614984998a941fee834aed814128838f5e76924b14e5409f3a565b53a66093ec676561ccb56b8ec",
        }))
        .unwrap();

        assert_eq!(header_from_hex, header_from_json);
        assert_eq!(
            header_from_hex.block_hash().to_string(),
            "00000000012860b68fc02b728ef20b64c2b15714c988d0664c54e0ace815b1bd"
        );
    }
}
