/// Utilities for decoding Polymer proof data
/// Includes helpers for parsing topics and ABI-decoding unindexed event data

/// Split concatenated topics bytes into individual 32-byte chunks
pub fn decode_topics(topics: &[u8]) -> Vec<[u8; 32]> {
    topics
        .chunks_exact(32)
        .map(|chunk| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(chunk);
            arr
        })
        .collect()
}

/// Extract Ethereum address from bytes32 (last 20 bytes)
pub fn bytes32_to_address(bytes: &[u8; 32]) -> [u8; 20] {
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&bytes[12..32]);
    addr
}

/// Extract u128 from bytes32
pub fn bytes32_to_u128(bytes: &[u8; 32]) -> u128 {
    u128::from_be_bytes([
        bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
    ])
}

/// Extract u64 from bytes32
pub fn bytes32_to_u64(bytes: &[u8; 32]) -> u64 {
    u64::from_be_bytes([
        bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
    ])
}

/// Parse string from ABI-encoded bytes
/// Format: offset (32 bytes) + length (32 bytes) + data (padded to 32-byte chunks)
pub fn decode_string_from_abi(data: &[u8], offset: usize) -> Result<String, String> {
    if data.len() < offset + 32 {
        return Err("Data too short for string length".to_string());
    }

    // Read length at offset
    let length = u64::from_be_bytes([
        data[offset + 24],
        data[offset + 25],
        data[offset + 26],
        data[offset + 27],
        data[offset + 28],
        data[offset + 29],
        data[offset + 30],
        data[offset + 31],
    ]) as usize;

    if data.len() < offset + 32 + length {
        return Err("Data too short for string content".to_string());
    }

    // Read string data
    let string_bytes = &data[offset + 32..offset + 32 + length];
    String::from_utf8(string_bytes.to_vec()).map_err(|e| format!("Invalid UTF-8: {}", e))
}

/// Parse address from ABI-encoded bytes at given offset
pub fn decode_address_from_abi(data: &[u8], offset: usize) -> Result<[u8; 20], String> {
    if data.len() < offset + 32 {
        return Err("Data too short for address".to_string());
    }

    let mut addr = [0u8; 20];
    addr.copy_from_slice(&data[offset + 12..offset + 32]);
    Ok(addr)
}

/// Parse u128 from ABI-encoded bytes at given offset
pub fn decode_u128_from_abi(data: &[u8], offset: usize) -> Result<u128, String> {
    if data.len() < offset + 32 {
        return Err("Data too short for u128".to_string());
    }

    Ok(u128::from_be_bytes([
        data[offset + 16],
        data[offset + 17],
        data[offset + 18],
        data[offset + 19],
        data[offset + 20],
        data[offset + 21],
        data[offset + 22],
        data[offset + 23],
        data[offset + 24],
        data[offset + 25],
        data[offset + 26],
        data[offset + 27],
        data[offset + 28],
        data[offset + 29],
        data[offset + 30],
        data[offset + 31],
    ]))
}

/// Parse u64 from ABI-encoded bytes at given offset
pub fn decode_u64_from_abi(data: &[u8], offset: usize) -> Result<u64, String> {
    if data.len() < offset + 32 {
        return Err("Data too short for u64".to_string());
    }

    Ok(u64::from_be_bytes([
        data[offset + 24],
        data[offset + 25],
        data[offset + 26],
        data[offset + 27],
        data[offset + 28],
        data[offset + 29],
        data[offset + 30],
        data[offset + 31],
    ]))
}

/// Parse u8 from ABI-encoded bytes at given offset
pub fn decode_u8_from_abi(data: &[u8], offset: usize) -> Result<u8, String> {
    if data.len() < offset + 32 {
        return Err("Data too short for u8".to_string());
    }

    Ok(data[offset + 31])
}
