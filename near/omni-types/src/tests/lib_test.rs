use near_sdk::borsh;
use near_sdk::json_types::U128;
use near_sdk::serde_json;

use crate::{
    stringify, ChainKind, Fee, OmniAddress, PayloadType, TransferId, TransferMessage, H160,
};
use std::str::FromStr;

#[test]
fn test_omni_address_serialization() {
    let address_str = "0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265";
    let address = OmniAddress::Eth(H160::from_str(address_str).unwrap());

    let serialized = serde_json::to_string(&address).unwrap();
    let deserialized = serde_json::from_str(&serialized).unwrap();

    assert_eq!(serialized, format!("\"eth:{address_str}\""));
    assert_eq!(address, deserialized);
}

#[test]
fn test_payload_prefix() {
    let res = borsh::to_vec(&PayloadType::TransferMessage).unwrap();
    assert_eq!(hex::encode(res), "00");
    let res = borsh::to_vec(&PayloadType::Metadata).unwrap();
    assert_eq!(hex::encode(res), "01");
    let res = borsh::to_vec(&PayloadType::ClaimNativeFee).unwrap();
    assert_eq!(hex::encode(res), "02");
}

#[test]
fn test_h160_from_str() {
    let addr = "5a08feed678c056650b3eb4a5cb1b9bb6f0fe265";
    let h160 = H160::from_str(addr).expect("Should parse without 0x prefix");
    assert_eq!(h160.to_string(), format!("0x{addr}"));

    let addr_with_prefix = format!("0x{addr}");
    let h160_with_prefix = H160::from_str(&addr_with_prefix).expect("Should parse with 0x prefix");
    assert_eq!(h160, h160_with_prefix);

    let invalid_hex = "0xnot_a_hex_string";
    let err = H160::from_str(invalid_hex).expect_err("Should fail with invalid hex");
    assert!(err.contains("ERR_INVALIDE_HEX"), "Error was: {err}");

    let short_addr = "0x5a08";
    let err = H160::from_str(short_addr).expect_err("Should fail with invalid length");
    assert!(err.contains("Invalid length:"), "Error was: {err}");
}

#[test]
fn test_eip_55_checksum() {
    let test_address = |input: &str, expected: &str, error_message: &str| {
        let h160 = H160::from_str(input).expect("Valid address");
        assert_eq!(
            h160.to_eip_55_checksum(),
            expected,
            "{error_message} {input} -> {expected}"
        );
    };

    let input = "0x5A08FeED678C056650b3eb4a5cb1b9BB6F0fE265";
    let expected = "5A08FeED678C056650b3eb4a5cb1b9BB6F0fE265";
    test_address(input, expected, "Original address");
    test_address(&input.to_lowercase(), expected, "Lowercase address");
    test_address(
        &format!("0x{}", expected.to_ascii_uppercase()),
        expected,
        "Uppercase address",
    );

    let input = "0x1234567890123456789012345678901234567890";
    let expected = "1234567890123456789012345678901234567890";
    test_address(input, expected, "No mixed case address");
}

#[test]
fn test_h160_deserialization() {
    let json = r#""0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265""#;
    let h160: H160 = serde_json::from_str(json).expect("Should deserialize with 0x prefix");
    assert_eq!(
        h160.to_string(),
        "0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265",
        "Should deserialize with 0x prefix"
    );

    let json = r#""5a08feed678c056650b3eb4a5cb1b9bb6f0fe265""#;
    let h160: H160 = serde_json::from_str(json).expect("Should deserialize without 0x prefix");
    assert_eq!(
        h160.to_string(),
        "0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265",
        "Should deserialize without 0x prefix"
    );

    let json = r#""0xnot_a_hex_string""#;
    let result: Result<H160, _> = serde_json::from_str(json);
    assert!(result.is_err(), "Should fail with invalid hex");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("ERR_INVALIDE_HEX"),
        "Error was: {err} but expected ERR_INVALIDE_HEX"
    );

    let json = r#""0x5a08""#;
    let result: Result<H160, _> = serde_json::from_str(json);
    assert!(result.is_err(), "Should fail with invalid length");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Invalid length"),
        "Error was: {err} but expected Invalid length"
    );

    let json = "123";
    let result: Result<H160, _> = serde_json::from_str(json);
    assert!(result.is_err(), "Should fail with non-string input");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid type"),
        "Error was: {err} but expected invalid type"
    );
}

#[test]
fn test_h160_serialization() {
    let addr = "5a08feed678c056650b3eb4a5cb1b9bb6f0fe265";
    let h160 = H160::from_str(addr).expect("Valid address");
    let serialized = serde_json::to_string(&h160).expect("Should serialize");
    assert_eq!(
        serialized, r#""0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265""#,
        "Invalid serialization."
    );

    let deserialized: H160 = serde_json::from_str(&serialized).expect("Should deserialize");
    assert_eq!(
        h160, deserialized,
        "Deserialization is not equal to initial value."
    );

    assert_eq!(
        format!(r#""{h160}""#),
        serialized,
        "Serialization does not preserve format from to_string()"
    );
}

#[test]
fn test_chain_kind_from_omni_address() {
    let test_chain_kind = |addr: OmniAddress, expected: ChainKind, chain_name: &str| {
        assert_eq!(
            ChainKind::from(&addr),
            expected,
            "Invalid chain kind from {chain_name} address"
        );
    };

    let evm_address =
        H160::from_str("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265").expect("Valid address");

    test_chain_kind(OmniAddress::Eth(evm_address.clone()), ChainKind::Eth, "ETH");
    test_chain_kind(
        OmniAddress::Near("alice.near".parse().unwrap()),
        ChainKind::Near,
        "NEAR",
    );
    test_chain_kind(
        OmniAddress::Sol("11111111111111111111111111111111".parse().unwrap()),
        ChainKind::Sol,
        "SOL",
    );
    test_chain_kind(OmniAddress::Arb(evm_address.clone()), ChainKind::Arb, "ARB");
    test_chain_kind(OmniAddress::Base(evm_address), ChainKind::Base, "BASE");
}

#[test]
fn test_omni_address_from_evm_address() {
    let evm_address =
        H160::from_str("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265").expect("Valid address");

    assert_eq!(
        OmniAddress::new_from_evm_address(ChainKind::Eth, evm_address.clone()),
        Ok(OmniAddress::Eth(evm_address.clone()))
    );

    for chain_kind in [ChainKind::Near, ChainKind::Sol] {
        let expected_error = format!("{chain_kind:?} is not an EVM chain");
        assert_eq!(
            OmniAddress::new_from_evm_address(chain_kind, evm_address.clone()),
            Err(expected_error)
        );
    }
}

#[test]
fn test_omni_address_from_str() {
    let evm_addr = "0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265";
    let test_cases = vec![
        (
            format!("eth:{evm_addr}"),
            Ok(OmniAddress::Eth(H160::from_str(evm_addr).unwrap())),
            "Should parse ETH address",
        ),
        (
            "near:alice.near".to_string(),
            Ok(OmniAddress::Near("alice.near".parse().unwrap())),
            "Should parse NEAR address",
        ),
        (
            "sol:11111111111111111111111111111111".to_string(),
            Ok(OmniAddress::Sol(
                "11111111111111111111111111111111".parse().unwrap(),
            )),
            "Should parse SOL address",
        ),
        (
            format!("arb:{evm_addr}"),
            Ok(OmniAddress::Arb(H160::from_str(evm_addr).unwrap())),
            "Should parse ARB address",
        ),
        (
            format!("base:{evm_addr}"),
            Ok(OmniAddress::Base(H160::from_str(evm_addr).unwrap())),
            "Should parse BASE address",
        ),
        (
            "invalid_format".to_string(),
            Err("ERR_INVALIDE_HEX".to_string()),
            "Should fail on missing chain prefix",
        ),
        (
            "unknown:address".to_string(),
            Err("Chain unknown is not supported".to_string()),
            "Should fail on unsupported chain",
        ),
    ];

    for (input, expected, message) in test_cases {
        let result = OmniAddress::from_str(&input);
        assert_eq!(result, expected, "{message}");
    }
}

#[test]
fn test_omni_address_display() {
    let evm_addr =
        H160::from_str("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265").expect("Valid EVM address");
    let test_cases = vec![
        (
            OmniAddress::Eth(evm_addr.clone()),
            format!("eth:{evm_addr}"),
            "ETH address should format as eth:0x...",
        ),
        (
            OmniAddress::Near("alice.near".parse().unwrap()),
            "near:alice.near".to_string(),
            "NEAR address should format as near:account",
        ),
        (
            OmniAddress::Sol("11111111111111111111111111111111".parse().unwrap()),
            "sol:11111111111111111111111111111111".to_string(),
            "SOL address should format as sol:address",
        ),
        (
            OmniAddress::Arb(evm_addr.clone()),
            format!("arb:{evm_addr}"),
            "ARB address should format as arb:0x...",
        ),
        (
            OmniAddress::Base(evm_addr.clone()),
            format!("base:{evm_addr}"),
            "BASE address should format as base:0x...",
        ),
    ];

    for (address, expected, message) in test_cases {
        assert_eq!(address.to_string(), expected, "{message}");
    }
}

#[test]
fn test_omni_address_visitor_expecting() {
    let invalid_value = 123;
    let expected_error =
        "invalid type: integer `123`, expected a string in the format 'chain:address'";
    let message = "Should show expecting message for integer input";

    let result: Result<OmniAddress, _> = serde_json::from_value(serde_json::json!(invalid_value));
    let error = result.unwrap_err().to_string();
    assert_eq!(error, expected_error, "{message}");
}

#[test]
fn test_fee_is_zero() {
    let test_cases = vec![
        (
            Fee {
                fee: U128(0),
                native_fee: U128(0),
            },
            true,
            "Should return true when both fees are zero",
        ),
        (
            Fee {
                fee: U128(1),
                native_fee: U128(0),
            },
            false,
            "Should return false when fee is non-zero",
        ),
        (
            Fee {
                fee: U128(0),
                native_fee: U128(1),
            },
            false,
            "Should return false when native_fee is non-zero",
        ),
        (
            Fee {
                fee: U128(1),
                native_fee: U128(1),
            },
            false,
            "Should return false when both fees are non-zero",
        ),
    ];

    for (fee, expected, message) in test_cases {
        assert_eq!(fee.is_zero(), expected, "{message}");
    }
}

#[test]
fn test_transfer_message_getters() {
    let evm_addr =
        H160::from_str("0x5a08feed678c056650b3eb4a5cb1b9bb6f0fe265").expect("Valid address");
    let test_cases = vec![
        (
            TransferMessage {
                destination_nonce: 1,
                origin_nonce: 123,
                token: OmniAddress::Near("token.near".parse().unwrap()),
                amount: U128(1000),
                recipient: OmniAddress::Near("bob.near".parse().unwrap()),
                fee: Fee::default(),
                sender: OmniAddress::Eth(evm_addr.clone()),
                msg: String::new(),
                origin_transfer_id: None,
            },
            ChainKind::Eth,
            TransferId {
                origin_chain: ChainKind::Eth,
                origin_nonce: 123,
            },
            "Should handle ETH sender",
        ),
        (
            TransferMessage {
                destination_nonce: 1,
                origin_nonce: 456,
                token: OmniAddress::Near("token.near".parse().unwrap()),
                amount: U128(2000),
                recipient: OmniAddress::Eth(evm_addr),
                fee: Fee::default(),
                sender: OmniAddress::Near("alice.near".parse().unwrap()),
                msg: String::new(),
                origin_transfer_id: None,
            },
            ChainKind::Near,
            TransferId {
                origin_chain: ChainKind::Near,
                origin_nonce: 456,
            },
            "Should handle NEAR sender",
        ),
        (
            TransferMessage {
                destination_nonce: 1,
                origin_nonce: 789,
                token: OmniAddress::Near("token.near".parse().unwrap()),
                amount: U128(3000),
                recipient: OmniAddress::Near("carol.near".parse().unwrap()),
                fee: Fee::default(),
                sender: OmniAddress::Sol("11111111111111111111111111111111".parse().unwrap()),
                msg: String::new(),
                origin_transfer_id: None,
            },
            ChainKind::Sol,
            TransferId {
                origin_chain: ChainKind::Sol,
                origin_nonce: 789,
            },
            "Should handle SOL sender",
        ),
    ];

    for (message, expected_chain, expected_id, error_msg) in test_cases {
        assert_eq!(message.get_origin_chain(), expected_chain, "{error_msg}");
        assert_eq!(message.get_transfer_id(), expected_id, "{error_msg}");
    }
}

#[test]
fn test_stringify() {
    assert_eq!(stringify(123), "123", "Should stringify integers");
    assert_eq!(stringify(42.5), "42.5", "Should stringify floats");
    assert_eq!(stringify(true), "true", "Should stringify booleans");
    assert_eq!(stringify('a'), "a", "Should stringify chars");

    #[allow(clippy::items_after_statements)]
    #[derive(Debug)]
    struct CustomType(i32);
    impl std::fmt::Display for CustomType {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "Custom({})", self.0)
        }
    }

    assert_eq!(
        stringify(CustomType(42)),
        "Custom(42)",
        "Should stringify custom types with Display implementation"
    );
}

#[test]
fn test_get_native_token_prefix() {
    for chain_kind in [
        ChainKind::Near,
        ChainKind::Sol,
        ChainKind::Base,
        ChainKind::Eth,
        ChainKind::Arb,
    ] {
        let prefix = OmniAddress::new_zero(chain_kind)
            .unwrap()
            .get_token_prefix();
        assert_eq!(
            prefix,
            chain_kind.as_ref().to_lowercase(),
            "Should return correct token prefix for {} chain",
            chain_kind.as_ref()
        );
    }
}

#[test]
fn test_get_evm_token_prefix() {
    let address = "0x23ddd3e3692d1861ed57ede224608875809e127f";
    let eth_address: OmniAddress = format!("eth:{address}").parse().unwrap();
    let prefix = eth_address.get_token_prefix();
    assert_eq!(prefix, "23ddd3e3692d1861ed57ede224608875809e127f");

    for chain_kind in [ChainKind::Base, ChainKind::Arb] {
        let chain_kind_prefix: String = chain_kind.as_ref().to_lowercase();
        let chain_address: OmniAddress = format!("{chain_kind_prefix}:{address}").parse().unwrap();
        assert_eq!(
            chain_address.get_token_prefix(),
            format!("{chain_kind_prefix}-{address}"),
        );
    }
}

#[test]
fn test_chain_kind_from_str() {
    let chain: ChainKind = "Eth".parse().unwrap();
    assert_eq!(chain, ChainKind::Eth);

    let chain: ChainKind = "Base".parse().unwrap();
    assert_eq!(chain, ChainKind::Base);
}
