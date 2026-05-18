#[test_only]
module omni_bridge::borsh_tests {
    use std::string;
    use omni_bridge::borsh;

    #[test]
    fun encode_u32_little_endian() {
        let bytes = borsh::encode_u32(0x12345678);
        // 0x12345678 little-endian = 78 56 34 12
        assert!(bytes.length() == 4, 100);
        assert!(bytes[0] == 0x78, 101);
        assert!(bytes[1] == 0x56, 102);
        assert!(bytes[2] == 0x34, 103);
        assert!(bytes[3] == 0x12, 104);
    }

    #[test]
    fun encode_u64_little_endian() {
        let bytes = borsh::encode_u64(0x0102030405060708);
        assert!(bytes.length() == 8, 110);
        assert!(bytes[0] == 0x08, 111);
        assert!(bytes[1] == 0x07, 112);
        assert!(bytes[7] == 0x01, 113);
    }

    #[test]
    fun encode_u128_little_endian() {
        let bytes = borsh::encode_u128(0x01020304050607080910111213141516);
        assert!(bytes.length() == 16, 120);
        assert!(bytes[0] == 0x16, 121);
        assert!(bytes[15] == 0x01, 122);
    }

    #[test]
    fun encode_string_length_prefixed() {
        let s = string::utf8(b"hello");
        let bytes = borsh::encode_string(&s);
        // 4-byte LE length (5) + "hello"
        assert!(bytes.length() == 9, 130);
        assert!(bytes[0] == 0x05, 131);
        assert!(bytes[1] == 0x00, 132);
        assert!(bytes[4] == 0x68, 133); // 'h'
        assert!(bytes[8] == 0x6f, 134); // 'o'
    }

    #[test]
    fun encode_empty_string() {
        let s = string::utf8(b"");
        let bytes = borsh::encode_string(&s);
        // 4-byte LE length (0)
        assert!(bytes.length() == 4, 140);
        assert!(bytes[0] == 0x00, 141);
        assert!(bytes[3] == 0x00, 142);
    }

    #[test]
    fun encode_address_32_bytes_be() {
        let addr: address = @0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20;
        let bytes = borsh::encode_address(addr);
        assert!(bytes.length() == 32, 150);
        assert!(bytes[0] == 0x01, 151);
        assert!(bytes[31] == 0x20, 152);
    }

    #[test]
    fun encode_byte_vec_length_prefixed() {
        let v = b"abc";
        let bytes = borsh::encode_byte_vec(&v);
        // 4-byte LE length (3) + "abc"
        assert!(bytes.length() == 7, 160);
        assert!(bytes[0] == 0x03, 161);
        assert!(bytes[4] == 0x61, 162); // 'a'
        assert!(bytes[6] == 0x63, 163); // 'c'
    }
}
