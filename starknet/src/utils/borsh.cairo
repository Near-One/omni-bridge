fn append_le_bytes(ref result: ByteArray, mut val: u128, byte_count: u32) {
    let mut i: u32 = 0;
    while i < byte_count {
        result.append_byte((val & 0xff).try_into().unwrap());
        val /= 0x100;
        i += 1;
    };
}

pub fn encode_u32(val: u32) -> ByteArray {
    let mut result: ByteArray = "";
    append_le_bytes(ref result, val.into(), 4);
    result
}

pub fn encode_u64(val: u64) -> ByteArray {
    let mut result: ByteArray = "";
    append_le_bytes(ref result, val.into(), 8);
    result
}

pub fn encode_u128(val: u128) -> ByteArray {
    let mut result: ByteArray = "";
    append_le_bytes(ref result, val, 16);
    result
}

pub fn encode_u256(val: u256) -> ByteArray {
    let mut result = encode_u128(val.low);
    result.append(@encode_u128(val.high));
    result
}

pub fn encode_byte_array(val: @ByteArray) -> ByteArray {
    let mut result = encode_u32(val.len());
    result.append(val);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_u32() {
        let val: u32 = 0x12345678;
        let encoded = encode_u32(val);
        let expected: ByteArray = "\x78\x56\x34\x12";

        assert_eq!(@encoded, @expected);
    }

    #[test]
    fn test_encode_u64() {
        let val: u64 = 0x0102030405060708;
        let encoded = encode_u64(val);
        let expected: ByteArray = "\x08\x07\x06\x05\x04\x03\x02\x01";

        assert_eq!(@encoded, @expected);
    }

    #[test]
    fn test_encode_u128() {
        let val: u128 = 0x01020304050607080910111213141516;
        let encoded = encode_u128(val);
        let expected: ByteArray =
            "\x16\x15\x14\x13\x12\x11\x10\x09\x08\x07\x06\x05\x04\x03\x02\x01";

        assert_eq!(@encoded, @expected);
    }

    #[test]
    fn test_encode_byte_array() {
        let val: ByteArray = "hello";
        let encoded = encode_byte_array(@val);
        let expected: ByteArray = "\x05\x00\x00\x00hello";

        assert_eq!(@encoded, @expected);
    }
}
