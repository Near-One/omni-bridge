#[test_only]
module omni_bridge::utils_tests;

use omni_bridge::utils;
use sui::sui::SUI;

// Vectors generated offline with secp256k1 key
// 0x4c0883a69102937d6231471b5dbb6204fe512961708279feb1be6ae5538da033
// signing keccak256(b"omni bridge test message") — the same construction the
// NEAR MPC uses over borsh payloads (signature emitted as r||s||(recid+27)).
fun test_message(): vector<u8> {
    b"omni bridge test message"
}

fun test_signer_address(): vector<u8> {
    vector[
        0xB9, 0x60, 0xBE, 0xD5, 0x3C, 0x17, 0xF9, 0xA0, 0x21, 0x53, 0x8B, 0x5D,
        0x6F, 0x08, 0xE7, 0x46, 0x6B, 0x96, 0x6C, 0x53,
    ]
}

fun test_signature(): vector<u8> {
    vector[
        0x98, 0xA0, 0xEA, 0x1B, 0xDD, 0x29, 0xDC, 0xC3, 0x14, 0x96, 0x82, 0x22,
        0xC0, 0x6B, 0x54, 0xB7, 0x20, 0xDE, 0x16, 0x6B, 0x65, 0x58, 0xCC, 0x4A,
        0xE7, 0x0B, 0x16, 0xCC, 0x80, 0x44, 0xDB, 0x41, 0x33, 0xCD, 0x7F, 0xAC,
        0x84, 0x34, 0x21, 0x62, 0x1F, 0x89, 0x90, 0x59, 0xDA, 0x98, 0x01, 0x1B,
        0xFD, 0xDF, 0xD0, 0x6A, 0xEF, 0x56, 0x04, 0x7D, 0xCA, 0x8C, 0xBC, 0x54,
        0xE0, 0x3C, 0xC5, 0xF8, 0x1C,
    ]
}

#[test]
fun verify_eth_signature_accepts_valid() {
    utils::verify_eth_signature(
        &test_message(),
        &test_signature(),
        &test_signer_address(),
    );
}

#[test]
fun verify_eth_signature_accepts_normalized_v() {
    // Same signature with v already normalized to {0,1}.
    let mut sig = test_signature();
    let last = sig.length() - 1;
    *(&mut sig[last]) = sig[last] - 27;
    utils::verify_eth_signature(&test_message(), &sig, &test_signer_address());
}

#[test]
#[expected_failure(abort_code = omni_bridge::utils::E_INVALID_SIGNATURE)]
fun verify_eth_signature_rejects_wrong_signer() {
    let mut wrong = test_signer_address();
    *(&mut wrong[0]) = 0x00;
    utils::verify_eth_signature(&test_message(), &test_signature(), &wrong);
}

#[test]
#[expected_failure]
fun verify_eth_signature_rejects_tampered_message() {
    // Tampering the message makes recovery yield a different (or no) key;
    // either way verification must abort.
    utils::verify_eth_signature(
        &b"omni bridge test messagX",
        &test_signature(),
        &test_signer_address(),
    );
}

#[test]
#[expected_failure]
fun verify_eth_signature_rejects_wrong_recovery_id() {
    // Same r||s with the other recovery id: recovers a different key (or
    // fails outright) — verification must abort either way.
    let mut sig = test_signature();
    let last = sig.length() - 1;
    *(&mut sig[last]) = 27; // vector was signed with v = 28
    utils::verify_eth_signature(&test_message(), &sig, &test_signer_address());
}

#[test]
#[expected_failure(abort_code = omni_bridge::utils::E_INVALID_SIGNATURE_LENGTH)]
fun verify_eth_signature_rejects_wrong_length() {
    let mut sig = test_signature();
    sig.pop_back();
    utils::verify_eth_signature(&test_message(), &sig, &test_signer_address());
}

#[test]
#[expected_failure(abort_code = omni_bridge::utils::E_INVALID_SIGNATURE)]
fun verify_eth_signature_rejects_short_expected_address() {
    let mut addr = test_signer_address();
    addr.pop_back();
    utils::verify_eth_signature(&test_message(), &test_signature(), &addr);
}

#[test]
fun normalize_decimals_clamps_at_nine() {
    assert!(utils::normalize_decimals(18) == 9);
    assert!(utils::normalize_decimals(9) == 9);
    assert!(utils::normalize_decimals(6) == 6);
    assert!(utils::normalize_decimals(0) == 0);
}

#[test]
fun coin_type_string_of_sui() {
    let s = utils::coin_type_string<SUI>();
    assert!(
        *s.as_bytes() ==
        b"0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
    );
}

#[test]
fun token_address_of_sui_is_keccak_of_type() {
    // keccak256(b"00...02::sui::SUI") computed offline.
    let expected = vector[
        0x66, 0x96, 0x38, 0x7A, 0xEC, 0xBB, 0x70, 0x52, 0x05, 0x02, 0x67, 0x83,
        0x04, 0x2F, 0x80, 0x38, 0x71, 0xC1, 0x90, 0x57, 0x0D, 0xD0, 0xA5, 0x78,
        0x82, 0xD9, 0xD3, 0x5E, 0xE0, 0xDF, 0x70, 0x0C,
    ];
    assert!(utils::token_address_bytes<SUI>() == expected);
    assert!(utils::token_address<SUI>().to_bytes() == expected);
}

#[test]
fun type_package_address_of_sui() {
    assert!(utils::type_package_address<SUI>() == @0x2);
}
