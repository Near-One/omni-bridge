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
    x"B960BED53C17F9A021538B5D6F08E7466B966C53"
}

fun test_signature(): vector<u8> {
    x"98A0EA1BDD29DCC314968222C06B54B720DE166B6558CC4AE70B16CC8044DB4133CD7FAC843421621F899059DA98011BFDDFD06AEF56047DCA8CBC54E03CC5F81C"
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
    let expected = x"6696387AECBB705205026783042F803871C190570DD0A57882D9D35EE0DF700C";
    assert!(utils::token_address_bytes<SUI>() == expected);
    assert!(utils::token_address<SUI>().to_bytes() == expected);
}

#[test]
fun type_package_address_of_sui() {
    assert!(utils::type_package_address<SUI>() == @0x2);
}
