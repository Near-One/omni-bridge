use core::integer::u128_byte_reverse;

pub mod borsh;


pub fn reverse_u256_bytes(value: u256) -> u256 {
    let low_reversed: u128 = u128_byte_reverse(value.low);
    let high_reversed: u128 = u128_byte_reverse(value.high);

    u256 { low: high_reversed, high: low_reversed }
}

pub fn felt252_to_string(value: felt252) -> ByteArray {
    let mut result: ByteArray = "";
    let mut val: u256 = value.into();

    // Extract ASCII characters (felt252 strings are encoded as big-endian ASCII)
    while val != 0 {
        let byte: u8 = (val % 256).try_into().unwrap();
        if byte != 0 {
            result.append_byte(byte);
        }
        val = val / 256;
    }

    // Reverse since we extracted from right to left
    let mut reversed: ByteArray = "";
    let len = result.len();
    let mut i = len;
    while i > 0 {
        i -= 1;
        reversed.append_byte(result[i]);
    }

    reversed
}
