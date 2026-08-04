#[test_only]
module omni_bridge::borsh_tests;

use omni_bridge::borsh;
use std::string;

#[test]
fun encode_byte_vec_empty() {
    let encoded = borsh::encode_byte_vec(&vector[]);
    assert!(encoded == vector[0, 0, 0, 0]);
}

#[test]
fun encode_byte_vec_short() {
    let encoded = borsh::encode_byte_vec(&vector[0xAA, 0xBB, 0xCC]);
    assert!(encoded == vector[3, 0, 0, 0, 0xAA, 0xBB, 0xCC]);
}

#[test]
fun encode_byte_vec_multibyte_length() {
    // 300 = 0x012C -> little-endian prefix [0x2C, 0x01, 0, 0].
    let mut payload = vector[];
    let mut i = 0u64;
    while (i < 300) {
        payload.push_back(0x11);
        i = i + 1;
    };
    let encoded = borsh::encode_byte_vec(&payload);
    assert!(encoded.length() == 304);
    assert!(encoded[0] == 0x2C);
    assert!(encoded[1] == 0x01);
    assert!(encoded[2] == 0);
    assert!(encoded[3] == 0);
    assert!(encoded[4] == 0x11);
    assert!(encoded[303] == 0x11);
}

#[test]
fun encode_string_hello() {
    let encoded = borsh::encode_string(&string::utf8(b"hello"));
    assert!(encoded == vector[5, 0, 0, 0, 104, 101, 108, 108, 111]);
}

#[test]
fun encode_string_empty() {
    let encoded = borsh::encode_string(&string::utf8(b""));
    assert!(encoded == vector[0, 0, 0, 0]);
}

