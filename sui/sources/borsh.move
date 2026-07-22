/// Borsh encoding helpers used to serialize cross-chain payloads.
///
/// Bridge payloads must be byte-identical to the encoding produced by the
/// NEAR side of the bridge so that the recovered signer matches
/// `near_bridge_derived_address`.
///
/// For fixed-width unsigned integers and `address`, Sui's native BCS
/// encoding is byte-identical to Borsh, so call sites use `bcs::to_bytes`
/// directly — no wrapper. Sequences are the exception: Borsh uses a fixed
/// 4-byte little-endian length prefix where BCS uses ULEB128, so the
/// helpers below encode that prefix explicitly.
module omni_bridge::borsh;

use std::string::String;

/// Borsh-style byte vector: 4-byte little-endian length + bytes.
public fun encode_byte_vec(val: &vector<u8>): vector<u8> {
    let len = val.length() as u32;
    let mut result = vector[
        (len & 0xFF) as u8,
        ((len >> 8) & 0xFF) as u8,
        ((len >> 16) & 0xFF) as u8,
        ((len >> 24) & 0xFF) as u8,
    ];
    result.append(*val);
    result
}

/// Borsh-style string: 4-byte little-endian length + UTF-8 bytes.
public fun encode_string(val: &String): vector<u8> {
    encode_byte_vec(val.as_bytes())
}
