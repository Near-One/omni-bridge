#[test_only]
module omni_bridge::borsh_tests {
    use std::string;
    use omni_bridge::borsh;

    // Fixed-width primitive encoding (`u32`/`u64`/`u128`/`address`) is
    // delegated to `bcs::to_bytes` at call sites; no need to retest the
    // framework. The tests below cover the Borsh-specific length-prefix
    // logic that BCS doesn't provide.

    #[test]
    fun encode_string_length_prefixed() {
        let s = string::utf8(b"hello");
        let bytes = borsh::encode_string(&s);
        // 4-byte LE length (5) + "hello"
        assert!(bytes.length() == 9, 130);
        assert!(bytes[0] == 0x05, 131);
        assert!(bytes[1] == 0x00, 132);
        assert!(bytes[2] == 0x00, 133);
        assert!(bytes[3] == 0x00, 134);
        assert!(bytes[4] == 0x68, 135); // 'h'
        assert!(bytes[8] == 0x6f, 136); // 'o'
    }

    #[test]
    fun encode_empty_string() {
        let s = string::utf8(b"");
        let bytes = borsh::encode_string(&s);
        // 4-byte LE length (0)
        assert!(bytes.length() == 4, 140);
        assert!(bytes[0] == 0x00, 141);
        assert!(bytes[1] == 0x00, 142);
        assert!(bytes[2] == 0x00, 143);
        assert!(bytes[3] == 0x00, 144);
    }

    #[test]
    fun encode_byte_vec_length_prefixed() {
        let v = b"abc";
        let bytes = borsh::encode_byte_vec(&v);
        // 4-byte LE length (3) + "abc"
        assert!(bytes.length() == 7, 160);
        assert!(bytes[0] == 0x03, 161);
        assert!(bytes[1] == 0x00, 162);
        assert!(bytes[2] == 0x00, 163);
        assert!(bytes[3] == 0x00, 164);
        assert!(bytes[4] == 0x61, 165); // 'a'
        assert!(bytes[5] == 0x62, 166); // 'b'
        assert!(bytes[6] == 0x63, 167); // 'c'
    }

    // Length prefix must use all 4 bytes — verify by encoding a vector
    // whose length doesn't fit in a single byte (catches any future
    // regression to a single-byte / ULEB128 prefix).
    #[test]
    fun encode_byte_vec_length_prefix_uses_all_four_bytes() {
        let v = vector[];
        let target_len: u32 = 0x1234; // 4660 bytes — fits in 2 LE bytes
        let i: u32 = 0;
        while (i < target_len) {
            v.push_back(((i & 0xff) as u8));
            i += 1;
        };
        let bytes = borsh::encode_byte_vec(&v);
        // Length 0x00001234 little-endian = 34 12 00 00
        assert!(bytes[0] == 0x34, 170);
        assert!(bytes[1] == 0x12, 171);
        assert!(bytes[2] == 0x00, 172);
        assert!(bytes[3] == 0x00, 173);
        assert!(bytes.length() == (target_len as u64) + 4, 174);
    }
}
