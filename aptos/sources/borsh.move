/// Borsh encoding helpers used to serialize cross-chain payloads.
///
/// Bridge payloads must be byte-identical to the encoding produced by the
/// NEAR side of the bridge so that the recovered signer matches
/// `near_bridge_derived_address`. Aptos addresses are 32 bytes and are
/// encoded big-endian (see `encode_address`).
module omni_bridge::borsh {
    use std::string::String;
    use std::bcs;

    public fun encode_u8(val: u8): vector<u8> {
        let bytes = vector[];
        bytes.push_back(val);
        bytes
    }

    public fun encode_u32(val: u32): vector<u8> {
        let bytes = vector[];
        let v = val;
        for (_i in 0..4) {
            bytes.push_back(((v & 0xff) as u8));
            v = v >> 8;
        };
        bytes
    }

    public fun encode_u64(val: u64): vector<u8> {
        let bytes = vector[];
        let v = val;
        for (_i in 0..8) {
            bytes.push_back(((v & 0xff) as u8));
            v = v >> 8;
        };
        bytes
    }

    public fun encode_u128(val: u128): vector<u8> {
        let bytes = vector[];
        let v = val;
        for (_i in 0..16) {
            bytes.push_back(((v & 0xff) as u8));
            v = v >> 8;
        };
        bytes
    }

    /// Encode an Aptos address as 32 bytes big-endian. BCS encodes
    /// addresses as raw big-endian bytes already, matching the 32-byte
    /// fixed-width encoding used by the Starknet variant of the bridge.
    public fun encode_address(addr: address): vector<u8> {
        bcs::to_bytes(&addr)
    }

    /// Borsh-style byte vector: 4-byte little-endian length + bytes.
    public fun encode_byte_vec(val: &vector<u8>): vector<u8> {
        let result = encode_u32((val.length() as u32));
        result.append(*val);
        result
    }

    /// Borsh-style string: 4-byte little-endian length + UTF-8 bytes.
    public fun encode_string(val: &String): vector<u8> {
        encode_byte_vec(val.bytes())
    }
}
