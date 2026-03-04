use crate::{
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage,
    },
    stringify, ChainKind, Fee, OmniAddress, H256,
};

/// Precomputed `sn_keccak("InitTransfer")` — keccak256 masked to 250 bits.
const INIT_TRANSFER_SELECTOR: [u8; 32] = compute_sn_keccak(b"InitTransfer");

/// Precomputed `sn_keccak("FinTransfer")`.
const FIN_TRANSFER_SELECTOR: [u8; 32] = compute_sn_keccak(b"FinTransfer");

/// Precomputed `sn_keccak("DeployToken")`.
const DEPLOY_TOKEN_SELECTOR: [u8; 32] = compute_sn_keccak(b"DeployToken");

/// Precomputed `sn_keccak("LogMetadata")`.
const LOG_METADATA_SELECTOR: [u8; 32] = compute_sn_keccak(b"LogMetadata");

/// Parses a Starknet log into an `InitTransferMessage`.
///
/// # Starknet `InitTransfer` event layout (from Cairo contract):
/// ```text
/// keys[0] = sn_keccak("InitTransfer")  (event selector)
/// keys[1] = sender                      (ContractAddress)
/// keys[2] = token_address               (ContractAddress)
/// keys[3] = origin_nonce                (u64 as felt)
/// data[0] = amount                      (u128 as felt)
/// data[1] = fee                         (u128 as felt)
/// data[2] = native_fee                  (u128 as felt)
/// data[3..] = recipient                 (ByteArray: serialized as felts)
/// data[..] = message                    (ByteArray: serialized as felts)
/// ```
pub fn parse_init_transfer(
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<InitTransferMessage, String> {
    if keys.len() < 4 {
        return Err(format!(
            "InitTransfer: expected at least 4 keys, got {}",
            keys.len()
        ));
    }
    if keys[0] != INIT_TRANSFER_SELECTOR {
        return Err("InitTransfer: selector mismatch".to_string());
    }

    let sender = OmniAddress::Strk(H256(keys[1]));
    let token = OmniAddress::Strk(H256(keys[2]));
    let origin_nonce = felt_to_u64(&keys[3])?;

    let mut cursor = FeltCursor::new(data);
    let amount = cursor.read_u128()?;
    let fee = cursor.read_u128()?;
    let native_fee = cursor.read_u128()?;
    let recipient_str = cursor.read_byte_array()?;
    let msg = cursor.read_byte_array()?;

    let emitter_address = OmniAddress::Strk(H256(*from_address));
    let recipient: OmniAddress = recipient_str.parse().map_err(stringify)?;

    Ok(InitTransferMessage {
        origin_nonce,
        token,
        amount: near_sdk::json_types::U128(amount),
        recipient,
        fee: Fee {
            fee: near_sdk::json_types::U128(fee),
            native_fee: near_sdk::json_types::U128(native_fee),
        },
        sender,
        msg,
        emitter_address,
    })
}

/// Parses a Starknet log into a `FinTransferMessage`.
///
/// # Starknet `FinTransfer` event layout:
/// ```text
/// keys[0] = sn_keccak("FinTransfer")
/// keys[1] = origin_chain                (u8 as felt)
/// keys[2] = origin_nonce                (u64 as felt)
/// data[0] = token_address               (ContractAddress as felt)
/// data[1] = amount                      (u128 as felt)
/// data[2] = recipient                   (ContractAddress as felt)
/// data[3..] = fee_recipient             (Option<ByteArray>)
/// data[..] = message                    (Option<ByteArray>)
/// ```
pub fn parse_fin_transfer(
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<FinTransferMessage, String> {
    if keys.len() < 3 {
        return Err(format!(
            "FinTransfer: expected at least 3 keys, got {}",
            keys.len()
        ));
    }
    if keys[0] != FIN_TRANSFER_SELECTOR {
        return Err("FinTransfer: selector mismatch".to_string());
    }

    let origin_chain: u8 = felt_to_u64(&keys[1])?.try_into().map_err(stringify)?;
    let origin_nonce = felt_to_u64(&keys[2])?;

    let mut cursor = FeltCursor::new(data);
    let _token_address = cursor.read_felt()?; // ContractAddress (not used in FinTransferMessage)
    let amount = cursor.read_u128()?;
    let _recipient = cursor.read_felt()?; // ContractAddress (not used in message directly)
    let fee_recipient_opt = cursor.read_option_byte_array()?;

    let emitter_address = OmniAddress::Strk(H256(*from_address));

    Ok(FinTransferMessage {
        transfer_id: crate::TransferId {
            origin_chain: origin_chain.try_into()?,
            origin_nonce,
        },
        amount: near_sdk::json_types::U128(amount),
        fee_recipient: fee_recipient_opt.and_then(|s| s.parse().ok()),
        emitter_address,
    })
}

/// Parses a Starknet log into a `DeployTokenMessage`.
///
/// # Starknet `DeployToken` event layout:
/// ```text
/// keys[0] = sn_keccak("DeployToken")
/// keys[1] = token_address               (ContractAddress)
/// data[0..] = near_token_id             (ByteArray)
/// data[..] = name                       (ByteArray)
/// data[..] = symbol                     (ByteArray)
/// data[..] = decimals                   (u8 as felt)
/// data[..] = origin_decimals            (u8 as felt)
/// ```
pub fn parse_deploy_token(
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<DeployTokenMessage, String> {
    if keys.len() < 2 {
        return Err(format!(
            "DeployToken: expected at least 2 keys, got {}",
            keys.len()
        ));
    }
    if keys[0] != DEPLOY_TOKEN_SELECTOR {
        return Err("DeployToken: selector mismatch".to_string());
    }

    let token_address = OmniAddress::Strk(H256(keys[1]));

    let mut cursor = FeltCursor::new(data);
    let near_token_id = cursor.read_byte_array()?;
    let _name = cursor.read_byte_array()?;
    let _symbol = cursor.read_byte_array()?;
    let decimals: u8 = cursor.read_u64()?.try_into().map_err(stringify)?;
    let origin_decimals: u8 = cursor.read_u64()?.try_into().map_err(stringify)?;

    let emitter_address = OmniAddress::Strk(H256(*from_address));

    Ok(DeployTokenMessage {
        token: near_token_id.parse().map_err(stringify)?,
        token_address,
        decimals,
        origin_decimals,
        emitter_address,
    })
}

/// Parses a Starknet log into a `LogMetadataMessage`.
///
/// # Starknet `LogMetadata` event layout:
/// ```text
/// keys[0] = sn_keccak("LogMetadata")
/// keys[1] = address                     (ContractAddress)
/// data[0..] = name                      (ByteArray)
/// data[..] = symbol                     (ByteArray)
/// data[..] = decimals                   (u8 as felt)
/// ```
pub fn parse_log_metadata(
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<LogMetadataMessage, String> {
    if keys.len() < 2 {
        return Err(format!(
            "LogMetadata: expected at least 2 keys, got {}",
            keys.len()
        ));
    }
    if keys[0] != LOG_METADATA_SELECTOR {
        return Err("LogMetadata: selector mismatch".to_string());
    }

    let token_address = OmniAddress::Strk(H256(keys[1]));

    let mut cursor = FeltCursor::new(data);
    let name = cursor.read_byte_array()?;
    let symbol = cursor.read_byte_array()?;
    let decimals: u8 = cursor.read_u64()?.try_into().map_err(stringify)?;

    let emitter_address = OmniAddress::Strk(H256(*from_address));

    Ok(LogMetadataMessage {
        token_address,
        name,
        symbol,
        decimals,
        emitter_address,
    })
}

/// Dispatches to the correct parser based on the event selector in `keys[0]`.
pub fn parse_starknet_event(
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<StarknetEvent, String> {
    if keys.is_empty() {
        return Err("Empty keys array — no event selector".to_string());
    }

    match keys[0] {
        INIT_TRANSFER_SELECTOR => {
            parse_init_transfer(from_address, keys, data).map(StarknetEvent::InitTransfer)
        }
        FIN_TRANSFER_SELECTOR => {
            parse_fin_transfer(from_address, keys, data).map(StarknetEvent::FinTransfer)
        }
        DEPLOY_TOKEN_SELECTOR => {
            parse_deploy_token(from_address, keys, data).map(StarknetEvent::DeployToken)
        }
        LOG_METADATA_SELECTOR => {
            parse_log_metadata(from_address, keys, data).map(StarknetEvent::LogMetadata)
        }
        _ => Err(format!("Unknown Starknet event selector: {:?}", &keys[0])),
    }
}

/// Dispatches to the correct parser based on `ProofKind`, validating that the event selector
/// matches the expected kind.
pub fn parse_starknet_proof(
    kind: crate::prover_result::ProofKind,
    _chain_kind: ChainKind,
    from_address: &[u8; 32],
    keys: &[[u8; 32]],
    data: &[[u8; 32]],
) -> Result<crate::prover_result::ProverResult, String> {
    use crate::prover_result::{ProofKind, ProverResult};

    match kind {
        ProofKind::InitTransfer => {
            parse_init_transfer(from_address, keys, data).map(ProverResult::InitTransfer)
        }
        ProofKind::FinTransfer => {
            parse_fin_transfer(from_address, keys, data).map(ProverResult::FinTransfer)
        }
        ProofKind::DeployToken => {
            parse_deploy_token(from_address, keys, data).map(ProverResult::DeployToken)
        }
        ProofKind::LogMetadata => {
            parse_log_metadata(from_address, keys, data).map(ProverResult::LogMetadata)
        }
    }
}

/// Parsed Starknet event variants.
pub enum StarknetEvent {
    InitTransfer(InitTransferMessage),
    FinTransfer(FinTransferMessage),
    DeployToken(DeployTokenMessage),
    LogMetadata(LogMetadataMessage),
}

/// A cursor over a slice of 32-byte felts for sequential reading.
struct FeltCursor<'a> {
    data: &'a [[u8; 32]],
    pos: usize,
}

impl<'a> FeltCursor<'a> {
    fn new(data: &'a [[u8; 32]]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_felt(&mut self) -> Result<[u8; 32], String> {
        if self.pos >= self.data.len() {
            return Err(format!(
                "FeltCursor: read past end at position {}",
                self.pos
            ));
        }
        let felt = self.data[self.pos];
        self.pos += 1;
        Ok(felt)
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        let felt = self.read_felt()?;
        felt_to_u64(&felt)
    }

    fn read_u128(&mut self) -> Result<u128, String> {
        let felt = self.read_felt()?;
        felt_to_u128(&felt)
    }

    /// Reads a Cairo `ByteArray` serialized as felts.
    ///
    /// Cairo `ByteArray` Serde layout:
    /// ```text
    /// felt[0]    = num_full_words (u32)
    /// felt[1..N] = full_words (each a felt containing 31 bytes of data)
    /// felt[N]    = pending_word (felt with remaining bytes, right-aligned)
    /// felt[N+1]  = pending_word_len (number of valid bytes in pending_word)
    /// ```
    #[allow(clippy::cast_possible_truncation)]
    fn read_byte_array(&mut self) -> Result<String, String> {
        let num_full_words = self.read_u64()? as usize;
        let mut bytes = Vec::with_capacity(num_full_words * 31 + 31);

        for _ in 0..num_full_words {
            let word = self.read_felt()?;
            bytes.extend_from_slice(&word[1..32]);
        }

        let pending_word = self.read_felt()?;
        let pending_len = self.read_u64()? as usize;

        if pending_len > 31 {
            return Err(format!(
                "ByteArray: pending_word_len {pending_len} exceeds 31"
            ));
        }

        if pending_len > 0 {
            let start = 32 - pending_len;
            bytes.extend_from_slice(&pending_word[start..32]);
        }

        String::from_utf8(bytes).map_err(|e| format!("ByteArray: invalid UTF-8: {e}"))
    }

    /// Reads a Cairo `Option<ByteArray>`.
    ///
    /// Serde layout: `0` for None, `1` followed by `ByteArray` for Some.
    fn read_option_byte_array(&mut self) -> Result<Option<String>, String> {
        let discriminant = self.read_u64()?;
        match discriminant {
            0 => Ok(None),
            1 => self.read_byte_array().map(Some),
            _ => Err(format!("Option: unexpected discriminant {discriminant}")),
        }
    }
}

fn felt_to_u64(felt: &[u8; 32]) -> Result<u64, String> {
    if felt[..24] != [0u8; 24] {
        return Err(format!("Felt value too large for u64: {felt:?}"));
    }
    Ok(u64::from_be_bytes(felt[24..32].try_into().unwrap()))
}

/// Interprets a 32-byte big-endian felt as a u128.
/// Fails if the value exceeds `u128::MAX`.
fn felt_to_u128(felt: &[u8; 32]) -> Result<u128, String> {
    if felt[..16] != [0u8; 16] {
        return Err(format!("Felt value too large for u128: {felt:?}"));
    }
    Ok(u128::from_be_bytes(felt[16..32].try_into().unwrap()))
}

/// Computes `sn_keccak(input)` at compile time.
/// `sn_keccak` = keccak256(input) with the top 6 bits cleared (250-bit truncation).
const fn compute_sn_keccak(input: &[u8]) -> [u8; 32] {
    let hash = const_keccak256(input);
    let mut result = hash;
    result[0] &= 0x03; // Clear top 6 bits
    result
}

/// Minimal const-compatible Keccak-256 implementation.
/// Based on the Keccak-f[1600] permutation.
const fn const_keccak256(input: &[u8]) -> [u8; 32] {
    let rate = 136; // rate in bytes for Keccak-256 (1088 bits / 8)
    let mut state = [0u64; 25];

    // Absorb phase: pad and process blocks
    let mut offset = 0;
    while offset + rate <= input.len() {
        state = xor_block(state, input, offset, rate);
        state = keccak_f1600(state);
        offset += rate;
    }

    // Last block with padding
    let remaining = input.len() - offset;
    let mut last_block = [0u8; 136];
    let mut i = 0;
    while i < remaining {
        last_block[i] = input[offset + i];
        i += 1;
    }
    // Keccak padding: 0x01 ... 0x80
    last_block[remaining] = 0x01;
    last_block[rate - 1] |= 0x80;

    state = xor_block(state, &last_block, 0, rate);
    state = keccak_f1600(state);

    // Squeeze: extract first 32 bytes
    let mut output = [0u8; 32];
    let mut j = 0;
    while j < 32 {
        let word = j / 8;
        let byte_pos = j % 8;
        #[allow(clippy::cast_possible_truncation)]
        {
            output[j] = (state[word] >> (8 * byte_pos)) as u8;
        }
        j += 1;
    }
    output
}

const fn xor_block(mut state: [u64; 25], data: &[u8], offset: usize, len: usize) -> [u64; 25] {
    let mut i = 0;
    while i < len / 8 {
        let base = offset + i * 8;
        let word = (data[base] as u64)
            | ((data[base + 1] as u64) << 8)
            | ((data[base + 2] as u64) << 16)
            | ((data[base + 3] as u64) << 24)
            | ((data[base + 4] as u64) << 32)
            | ((data[base + 5] as u64) << 40)
            | ((data[base + 6] as u64) << 48)
            | ((data[base + 7] as u64) << 56);
        state[i] ^= word;
        i += 1;
    }
    state
}

const fn keccak_f1600(mut state: [u64; 25]) -> [u64; 25] {
    const RC: [u64; 24] = [
        0x0000_0000_0000_0001,
        0x0000_0000_0000_8082,
        0x8000_0000_0000_808A,
        0x8000_0000_8000_8000,
        0x0000_0000_0000_808B,
        0x0000_0000_8000_0001,
        0x8000_0000_8000_8081,
        0x8000_0000_0000_8009,
        0x0000_0000_0000_008A,
        0x0000_0000_0000_0088,
        0x0000_0000_8000_8009,
        0x0000_0000_8000_000A,
        0x0000_0000_8000_808B,
        0x8000_0000_0000_008B,
        0x8000_0000_0000_8089,
        0x8000_0000_0000_8003,
        0x8000_0000_0000_8002,
        0x8000_0000_0000_0080,
        0x0000_0000_0000_800A,
        0x8000_0000_8000_000A,
        0x8000_0000_8000_8081,
        0x8000_0000_0000_8080,
        0x0000_0000_8000_0001,
        0x8000_0000_8000_8008,
    ];
    const ROTATIONS: [u32; 25] = [
        0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56,
        14,
    ];
    const PI: [usize; 25] = [
        0, 10, 20, 5, 15, 16, 1, 11, 21, 6, 7, 17, 2, 12, 22, 23, 8, 18, 3, 13, 14, 24, 9, 19, 4,
    ];

    let mut round = 0;
    while round < 24 {
        // θ step
        let mut c = [0u64; 5];
        let mut x = 0;
        while x < 5 {
            c[x] = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
            x += 1;
        }
        let mut d = [0u64; 5];
        x = 0;
        while x < 5 {
            d[x] = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            x += 1;
        }
        x = 0;
        while x < 25 {
            state[x] ^= d[x % 5];
            x += 1;
        }

        // ρ and π steps
        let mut temp = [0u64; 25];
        x = 0;
        while x < 25 {
            temp[PI[x]] = state[x].rotate_left(ROTATIONS[x]);
            x += 1;
        }

        // χ step
        x = 0;
        while x < 25 {
            let y_base = (x / 5) * 5;
            state[x] = temp[x] ^ (!temp[y_base + (x + 1) % 5] & temp[y_base + (x + 2) % 5]);
            x += 1;
        }

        // ι step
        state[0] ^= RC[round];
        round += 1;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const_keccak256_empty() {
        // keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        let result = const_keccak256(b"");
        let expected =
            hex::decode("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
                .unwrap();
        assert_eq!(&result[..], &expected[..], "keccak256('') mismatch");
    }

    #[test]
    fn test_const_keccak256_hello() {
        // keccak256("hello") = 1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8
        let result = const_keccak256(b"hello");
        let expected =
            hex::decode("1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8")
                .unwrap();
        assert_eq!(&result[..], &expected[..], "keccak256('hello') mismatch");
    }

    #[test]
    fn test_sn_keccak_selectors() {
        // Verify that our const-computed selectors match runtime sha3::Keccak256
        use sha3::{Digest, Keccak256};

        for (name, expected) in [
            ("InitTransfer", INIT_TRANSFER_SELECTOR),
            ("FinTransfer", FIN_TRANSFER_SELECTOR),
            ("DeployToken", DEPLOY_TOKEN_SELECTOR),
            ("LogMetadata", LOG_METADATA_SELECTOR),
        ] {
            let mut hash = Keccak256::digest(name.as_bytes()).to_vec();
            hash[0] &= 0x03;
            assert_eq!(&hash[..], &expected[..], "sn_keccak({name}) mismatch");
        }
    }

    #[test]
    fn test_felt_to_u64() {
        let mut felt = [0u8; 32];
        felt[31] = 42;
        assert_eq!(felt_to_u64(&felt).unwrap(), 42);

        felt[24] = 1;
        felt[31] = 0;
        assert_eq!(felt_to_u64(&felt).unwrap(), 1 << 56);

        // Too large
        let mut big = [0u8; 32];
        big[23] = 1;
        assert!(felt_to_u64(&big).is_err());
    }

    #[test]
    fn test_felt_to_u128() {
        let mut felt = [0u8; 32];
        felt[31] = 100;
        assert_eq!(felt_to_u128(&felt).unwrap(), 100);

        // Too large
        let mut big = [0u8; 32];
        big[15] = 1;
        assert!(felt_to_u128(&big).is_err());
    }

    /// Helper to create a felt from a u64 value.
    fn u64_felt(val: u64) -> [u8; 32] {
        let mut felt = [0u8; 32];
        let bytes = val.to_be_bytes();
        felt[24..32].copy_from_slice(&bytes);
        felt
    }

    /// Helper to create a felt from a u128 value.
    fn u128_felt(val: u128) -> [u8; 32] {
        let mut felt = [0u8; 32];
        let bytes = val.to_be_bytes();
        felt[16..32].copy_from_slice(&bytes);
        felt
    }

    /// Helper to create a felt from a 32-byte hex string (no 0x prefix).
    fn hex_felt(hex_str: &str) -> [u8; 32] {
        let bytes = hex::decode(hex_str).unwrap();
        let mut felt = [0u8; 32];
        let start = 32 - bytes.len();
        felt[start..32].copy_from_slice(&bytes);
        felt
    }

    /// Encode a Cairo `ByteArray` into felts.
    /// Each full word is 31 bytes right-padded in the felt (stored in bytes[1..32]).
    fn encode_byte_array(s: &str) -> Vec<[u8; 32]> {
        let bytes = s.as_bytes();
        let num_full_words = bytes.len() / 31;
        let pending_len = bytes.len() % 31;

        let mut felts = Vec::new();
        // num_full_words
        felts.push(u64_felt(num_full_words as u64));
        // full words
        for i in 0..num_full_words {
            let mut word = [0u8; 32];
            word[1..32].copy_from_slice(&bytes[i * 31..(i + 1) * 31]);
            felts.push(word);
        }
        // pending_word
        let mut pending = [0u8; 32];
        if pending_len > 0 {
            let start = 32 - pending_len;
            pending[start..32].copy_from_slice(&bytes[num_full_words * 31..]);
        }
        felts.push(pending);
        // pending_word_len
        felts.push(u64_felt(pending_len as u64));

        felts
    }

    #[test]
    fn test_byte_array_encoding_roundtrip() {
        let test_str = "near:frolik.testnet";
        let felts = encode_byte_array(test_str);
        let mut cursor = FeltCursor::new(&felts);
        let decoded = cursor.read_byte_array().unwrap();
        assert_eq!(decoded, test_str);
    }

    #[test]
    fn test_byte_array_empty() {
        let felts = encode_byte_array("");
        let mut cursor = FeltCursor::new(&felts);
        let decoded = cursor.read_byte_array().unwrap();
        assert_eq!(decoded, "");
    }

    #[test]
    fn test_byte_array_long_string() {
        // 62 chars = 2 full words (2 * 31) + 0 pending
        let long_str = "a]".repeat(31);
        let felts = encode_byte_array(&long_str);
        let mut cursor = FeltCursor::new(&felts);
        let decoded = cursor.read_byte_array().unwrap();
        assert_eq!(decoded, long_str);
    }

    #[test]
    fn test_parse_init_transfer() {
        let sender = hex_felt("0000000000000000000000000000000000000000000000000000000000aa0001");
        let token_addr =
            hex_felt("0000000000000000000000000000000000000000000000000000000000bb0002");
        let emitter = hex_felt("0000000000000000000000000000000000000000000000000000000000cc0003");

        let keys = vec![
            INIT_TRANSFER_SELECTOR,
            sender,
            token_addr,
            u64_felt(7), // origin_nonce
        ];

        let mut data = Vec::new();
        data.push(u128_felt(1000)); // amount
        data.push(u128_felt(10)); // fee
        data.push(u128_felt(5)); // native_fee
        data.extend(encode_byte_array("near:frolik.testnet")); // recipient
        data.extend(encode_byte_array("")); // message

        let msg = parse_init_transfer(&emitter, &keys, &data).unwrap();
        assert_eq!(msg.origin_nonce, 7);
        assert_eq!(msg.amount.0, 1000);
        assert_eq!(msg.fee.fee.0, 10);
        assert_eq!(msg.fee.native_fee.0, 5);
        assert_eq!(msg.msg, "");
        assert_eq!(msg.emitter_address, OmniAddress::Strk(H256(emitter)));
        assert_eq!(msg.sender, OmniAddress::Strk(H256(sender)));
        assert_eq!(msg.token, OmniAddress::Strk(H256(token_addr)));
    }

    #[test]
    fn test_parse_log_metadata() {
        let token_addr =
            hex_felt("0000000000000000000000000000000000000000000000000000000000dd0001");
        let emitter = hex_felt("0000000000000000000000000000000000000000000000000000000000ee0002");

        let keys = vec![LOG_METADATA_SELECTOR, token_addr];

        let mut data = Vec::new();
        data.extend(encode_byte_array("Wrapped ETH")); // name
        data.extend(encode_byte_array("WETH")); // symbol
        data.push(u64_felt(18)); // decimals

        let msg = parse_log_metadata(&emitter, &keys, &data).unwrap();
        assert_eq!(msg.name, "Wrapped ETH");
        assert_eq!(msg.symbol, "WETH");
        assert_eq!(msg.decimals, 18);
        assert_eq!(msg.token_address, OmniAddress::Strk(H256(token_addr)));
        assert_eq!(msg.emitter_address, OmniAddress::Strk(H256(emitter)));
    }

    #[test]
    fn test_parse_deploy_token() {
        let token_addr =
            hex_felt("0000000000000000000000000000000000000000000000000000000000dd0001");
        let emitter = hex_felt("0000000000000000000000000000000000000000000000000000000000ee0002");

        let keys = vec![DEPLOY_TOKEN_SELECTOR, token_addr];

        let mut data = Vec::new();
        data.extend(encode_byte_array("wrap.testnet")); // near_token_id
        data.extend(encode_byte_array("Wrapped ETH")); // name
        data.extend(encode_byte_array("WETH")); // symbol
        data.push(u64_felt(18)); // decimals
        data.push(u64_felt(18)); // origin_decimals

        let msg = parse_deploy_token(&emitter, &keys, &data).unwrap();
        assert_eq!(msg.token.to_string(), "wrap.testnet");
        assert_eq!(msg.decimals, 18);
        assert_eq!(msg.origin_decimals, 18);
        assert_eq!(msg.token_address, OmniAddress::Strk(H256(token_addr)));
    }

    #[test]
    fn test_parse_fin_transfer() {
        let emitter = hex_felt("0000000000000000000000000000000000000000000000000000000000ff0001");
        let token_addr =
            hex_felt("0000000000000000000000000000000000000000000000000000000000aa0001");
        let recipient =
            hex_felt("0000000000000000000000000000000000000000000000000000000000bb0001");

        let keys = vec![
            FIN_TRANSFER_SELECTOR,
            u64_felt(ChainKind::Eth as u64), // origin_chain
            u64_felt(42),                    // origin_nonce
        ];

        let mut data = vec![
            token_addr,     // token_address
            u128_felt(500), // amount
            recipient,      // recipient
            u64_felt(1),    // fee_recipient:
        ];
        data.extend(encode_byte_array("fee.testnet"));
        // message: None
        data.push(u64_felt(0)); // None discriminant

        let msg = parse_fin_transfer(&emitter, &keys, &data).unwrap();
        assert_eq!(msg.transfer_id.origin_chain, ChainKind::Eth);
        assert_eq!(msg.transfer_id.origin_nonce, 42);
        assert_eq!(msg.amount.0, 500);
        assert_eq!(msg.fee_recipient.unwrap().to_string(), "fee.testnet");
        assert_eq!(msg.emitter_address, OmniAddress::Strk(H256(emitter)));
    }

    #[test]
    fn test_parse_starknet_event_dispatches() {
        let emitter = [0x11u8; 32];
        let token_addr = [0x22u8; 32];

        let keys = vec![LOG_METADATA_SELECTOR, token_addr];
        let mut data = Vec::new();
        data.extend(encode_byte_array("TestToken"));
        data.extend(encode_byte_array("TT"));
        data.push(u64_felt(8));

        let result = parse_starknet_event(&emitter, &keys, &data).unwrap();
        match result {
            StarknetEvent::LogMetadata(msg) => {
                assert_eq!(msg.name, "TestToken");
                assert_eq!(msg.symbol, "TT");
                assert_eq!(msg.decimals, 8);
            }
            _ => panic!("Expected LogMetadata variant"),
        }
    }

    #[test]
    fn test_parse_starknet_event_unknown_selector() {
        let emitter = [0x11u8; 32];
        let keys = vec![[0xffu8; 32]]; // Unknown selector
        let data = vec![];

        let result = parse_starknet_event(&emitter, &keys, &data);
        assert!(result.is_err());
    }
}
