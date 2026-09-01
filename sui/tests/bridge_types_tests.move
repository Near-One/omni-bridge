#[test_only]
module omni_bridge::bridge_types_tests;

use omni_bridge::bridge_types;
use std::string;

const CHAIN_ID: u8 = 14;

fun token_address(): address {
    @0x1111111111111111111111111111111111111111111111111111111111111111
}

fun recipient(): address {
    @0x2222222222222222222222222222222222222222222222222222222222222222
}

#[test]
fun metadata_payload_layout() {
    let payload = bridge_types::new_metadata_payload(
        string::utf8(b"wrap.near"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    let encoded = payload.metadata_to_borsh();

    // 0x01 | str(9) | str(12) | str(5) | u8 = 1 + 13 + 16 + 9 + 1 = 40.
    assert!(encoded.length() == 40);
    assert!(encoded[0] == 1); // PayloadType::Metadata
    assert!(encoded[1] == 9 && encoded[2] == 0 && encoded[3] == 0 && encoded[4] == 0);
    assert!(encoded[5] == 0x77); // 'w' of "wrap.near"
    assert!(encoded[14] == 12 && encoded[15] == 0);
    assert!(encoded[18] == 0x57); // 'W' of "Wrapped NEAR"
    assert!(encoded[30] == 5 && encoded[31] == 0);
    assert!(encoded[34] == 0x77); // 'w' of "wNEAR"
    assert!(encoded[39] == 24); // decimals
}

#[test]
fun transfer_message_layout_with_fee_recipient() {
    let payload = bridge_types::new_transfer_message_payload(
        42, // destination_nonce
        1, // origin_chain (Near)
        7, // origin_nonce
        token_address(),
        1_000_000, // amount
        recipient(),
        option::some(string::utf8(b"relayer.near")),
        vector[], // empty message contributes nothing
    );
    let encoded = payload.transfer_message_to_borsh(CHAIN_ID);

    // 1 + 8 + 1 + 8 + 1 + 32 + 16 + 1 + 32 + 1 + 4 + 12 = 117.
    assert!(encoded.length() == 117);
    assert!(encoded[0] == 0); // PayloadType::TransferMessage
    // destination_nonce u64 LE
    assert!(encoded[1] == 42 && encoded[2] == 0 && encoded[8] == 0);
    assert!(encoded[9] == 1); // origin_chain
    assert!(encoded[10] == 7 && encoded[17] == 0); // origin_nonce u64 LE
    assert!(encoded[18] == CHAIN_ID); // OmniAddress tag for token_address
    assert!(encoded[19] == 0x11 && encoded[50] == 0x11); // 32-byte token address
    // amount u128 LE: 1_000_000 = 0x0F4240
    assert!(encoded[51] == 0x40 && encoded[52] == 0x42 && encoded[53] == 0x0F);
    assert!(encoded[54] == 0 && encoded[66] == 0);
    assert!(encoded[67] == CHAIN_ID); // OmniAddress tag for recipient
    assert!(encoded[68] == 0x22 && encoded[99] == 0x22); // 32-byte recipient
    assert!(encoded[100] == 1); // fee_recipient Option tag: Some
    assert!(encoded[101] == 12 && encoded[102] == 0); // fee_recipient length
    assert!(encoded[105] == 0x72); // 'r' of "relayer.near"
    assert!(encoded[116] == 0x72); // 'r' of "...near"... last byte is 'r'
}

#[test]
fun transfer_message_layout_no_fee_recipient() {
    let payload = bridge_types::new_transfer_message_payload(
        42,
        1,
        7,
        token_address(),
        1_000_000,
        recipient(),
        option::none(),
        vector[],
    );
    let encoded = payload.transfer_message_to_borsh(CHAIN_ID);

    // 1 + 8 + 1 + 8 + 1 + 32 + 16 + 1 + 32 + 1 = 101.
    assert!(encoded.length() == 101);
    assert!(encoded[100] == 0); // fee_recipient Option tag: None
}

#[test]
fun transfer_message_appends_untagged_message() {
    let payload = bridge_types::new_transfer_message_payload(
        42,
        1,
        7,
        token_address(),
        1_000_000,
        recipient(),
        option::none(),
        vector[0xDE, 0xAD],
    );
    let encoded = payload.transfer_message_to_borsh(CHAIN_ID);

    // No Option tag for message: just u32-LE length + bytes.
    assert!(encoded.length() == 107);
    assert!(encoded[100] == 0); // fee_recipient None
    assert!(encoded[101] == 2 && encoded[102] == 0 && encoded[103] == 0 && encoded[104] == 0);
    assert!(encoded[105] == 0xDE && encoded[106] == 0xAD);
}

#[test]
fun accessors_round_trip() {
    let payload = bridge_types::new_transfer_message_payload(
        1,
        2,
        3,
        token_address(),
        4,
        recipient(),
        option::some(string::utf8(b"fee.near")),
        vector[0x01],
    );
    assert!(payload.transfer_fee_recipient() == option::some(string::utf8(b"fee.near")));
    assert!(payload.transfer_message() == vector[0x01]);

    let metadata = bridge_types::new_metadata_payload(
        string::utf8(b"t"),
        string::utf8(b"n"),
        string::utf8(b"s"),
        8,
    );
    assert!(metadata.metadata_token() == string::utf8(b"t"));
    assert!(metadata.metadata_name() == string::utf8(b"n"));
    assert!(metadata.metadata_symbol() == string::utf8(b"s"));
    assert!(metadata.metadata_decimals() == 8);
}
