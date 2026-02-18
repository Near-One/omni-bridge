pub fn encode_u32(val: u32) -> ByteArray {
    let mut result: ByteArray = Default::default();
    let v: u128 = val.into();
    result.append_byte((v & 0xff).try_into().unwrap());
    result.append_byte(((v / 0x100) & 0xff).try_into().unwrap());
    result.append_byte(((v / 0x10000) & 0xff).try_into().unwrap());
    result.append_byte(((v / 0x1000000) & 0xff).try_into().unwrap());
    result
}

pub fn encode_u64(val: u64) -> ByteArray {
    let mut result: ByteArray = Default::default();
    result.append_byte((val & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000000000) & 0xff).try_into().unwrap());
    result
}

pub fn encode_u128(val: u128) -> ByteArray {
    let mut result: ByteArray = Default::default();
    result.append_byte((val & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100000000000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000000000000000000000000000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000000000000000000000000000) & 0xff).try_into().unwrap());
    result
}

pub fn encode_address(val: starknet::ContractAddress) -> ByteArray {
    let felt_val: felt252 = val.into();
    let u256_val: u256 = felt_val.into();
    let mut result: ByteArray = Default::default();
    result.append_word(u256_val.high.into(), 16);
    result.append_word(u256_val.low.into(), 16);
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

    #[test]
    fn test_encode_address() {
        // Test with address 0x123 (291 in decimal)
        // As u256: low = 0x123, high = 0
        // As 32 bytes BE: [0x00, ..., 0x00] (30 bytes) + [0x01, 0x23]
        let addr: starknet::ContractAddress = 0x123.try_into().unwrap();
        let encoded = encode_address(addr);

        let mut expected: ByteArray = "";
        let mut i: u32 = 0;
        while i < 30 {
            expected.append_byte(0);
            i += 1;
        }
        expected.append_byte(0x01);
        expected.append_byte(0x23);

        assert_eq!(@encoded, @expected);
    }

    #[test]
    fn test_encode_address_max() {
        // Test with a full 32-byte address value (64 hex characters)
        let addr: starknet::ContractAddress =
            0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
            .try_into()
            .unwrap();
        let encoded = encode_address(addr);

        // Expected: 32 bytes BE encoding of the full 256-bit address
        let expected: ByteArray =
            "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20";

        assert_eq!(@encoded, @expected);
    }
}
