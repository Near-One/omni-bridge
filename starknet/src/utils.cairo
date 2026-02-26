use core::integer::u128_byte_reverse;

pub mod borsh;


pub fn reverse_u256_bytes(value: u256) -> u256 {
    let low_reversed: u128 = u128_byte_reverse(value.low);
    let high_reversed: u128 = u128_byte_reverse(value.high);

    u256 { low: high_reversed, high: low_reversed }
}

pub fn felt252_to_string(value: felt252) -> ByteArray {
    let mut result: ByteArray = Default::default();
    let mut len: usize = 0;
    let mut val: u256 = value.into();
    while val != 0 {
        len += 1;
        val /= 256;
    }
    if len > 0 {
        result.append_word(value, len);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::felt252_to_string;

    #[test]
    fn test_empty() {
        let result = felt252_to_string(0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_single_char() {
        // 'A' = 0x41
        let result = felt252_to_string(0x41);
        assert_eq!(result, "A");
    }

    #[test]
    fn test_short_string() {
        // 'ETH' = 0x455448
        let result = felt252_to_string(0x455448);
        assert_eq!(result, "ETH");
    }

    #[test]
    fn test_longer_string() {
        // 'Hello' = 0x48656c6c6f
        let result = felt252_to_string(0x48656c6c6f);
        assert_eq!(result, "Hello");
    }
}
