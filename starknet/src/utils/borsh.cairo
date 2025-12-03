use core::option::OptionTrait;
use core::traits::TryInto;

pub fn encode_u32(val: u32) -> ByteArray {
    let mut result: ByteArray = "";
    result.append_byte((val & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x100) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x10000) & 0xff).try_into().unwrap());
    result.append_byte(((val / 0x1000000) & 0xff).try_into().unwrap());
    result
}

pub fn encode_u64(val: u64) -> ByteArray {
    let mut result: ByteArray = "";
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
    let mut result: ByteArray = "";
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

pub fn encode_u256(val: u256) -> ByteArray {
    let mut result: ByteArray = "";

    let low_bytes = encode_u128(val.low);
    let mut i = 0;
    while i < low_bytes.len() {
        result.append_byte(low_bytes[i]);
        i += 1;
    }

    let high_bytes = encode_u128(val.high);
    let mut i = 0;
    while i < high_bytes.len() {
        result.append_byte(high_bytes[i]);
        i += 1;
    }
    result
}

pub fn encode_byte_array(val: @ByteArray) -> ByteArray {
    let mut result: ByteArray = "";

    // Encode length as u32 little-endian
    let len: u32 = val.len();
    let len_bytes = encode_u32(len);
    let mut i = 0;
    while i < len_bytes.len() {
        result.append_byte(len_bytes[i]);
        i += 1;
    }

    // Append string bytes
    let mut i = 0;
    while i < val.len() {
        result.append_byte(val[i]);
        i += 1;
    }

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
