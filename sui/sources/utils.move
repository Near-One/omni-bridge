/// Shared utilities for the Omni Bridge: Ethereum-style signature
/// verification (used to validate MPC-signed payloads from NEAR), decimal
/// normalization, and coin-type identity helpers.
module omni_bridge::utils;

use std::string::{Self, String};
use std::type_name;
use sui::address;
use sui::ecdsa_k1;
use sui::hash;

/// Signature payload could not be parsed.
#[allow(unused_const)]
const E_INVALID_SIGNATURE_LENGTH: u64 = 1;
/// Recovered Ethereum address does not match the expected signer (also used
/// for malformed expected addresses).
#[allow(unused_const)]
const E_INVALID_SIGNATURE: u64 = 3;

/// Decimals are clamped to 9 (native SUI precision) because Sui `Coin`
/// amounts are `u64` (max ~1.84e19).
const MAX_ALLOWED_DECIMALS: u8 = 9;

/// Cap decimals at the protocol-wide maximum.
public fun normalize_decimals(decimals: u8): u8 {
    if (decimals > MAX_ALLOWED_DECIMALS) {
        MAX_ALLOWED_DECIMALS
    } else {
        decimals
    }
}

/// Verify an Ethereum-style 65-byte signature (`r || s || v`) over
/// `message_bytes`.
///
/// `v` is the Ethereum recovery id as emitted by the NEAR MPC
/// (`recovery_id + 27`); 0/1 are also accepted. Sui's
/// `secp256k1_ecrecover` hashes the message internally (flag 0 =
/// keccak256), so `message_bytes` is the RAW borsh payload — never a
/// digest. The Ethereum address is the last 20 bytes of
/// `keccak256(uncompressed_pubkey[1..65])`.
public fun verify_eth_signature(
    message_bytes: &vector<u8>,
    signature: &vector<u8>,
    expected_address: &vector<u8>,
) {
    assert!(signature.length() == 65, E_INVALID_SIGNATURE_LENGTH);
    assert!(expected_address.length() == 20, E_INVALID_SIGNATURE);

    let mut sig = *signature;
    let v = &mut sig[64];
    if (*v >= 27) {
        *v = *v - 27;
    };

    let compressed = ecdsa_k1::secp256k1_ecrecover(&sig, message_bytes, 0);
    let uncompressed = ecdsa_k1::decompress_pubkey(&compressed);

    // Skip the 0x04 prefix; hash the raw 64-byte public key.
    let mut pk64 = vector[];
    let mut i = 1;
    while (i < 65) {
        pk64.push_back(uncompressed[i]);
        i = i + 1;
    };
    let digest = hash::keccak256(&pk64);

    let mut addr = vector[];
    let mut j = 12;
    while (j < 32) {
        addr.push_back(digest[j]);
        j = j + 1;
    };

    assert!(addr == *expected_address, E_INVALID_SIGNATURE);
}

/// Canonical coin-type string for `T`: 64 lowercase hex chars (no `0x`)
/// of the DEFINING package id, then `::module::Name`. This exact form is
/// the preimage of the 32-byte token id used on the wire.
public fun coin_type_string<T>(): String {
    string::from_ascii(type_name::with_defining_ids<T>().into_string())
}

/// 32-byte wire identity of coin type `T`:
/// `keccak256(coin_type_string<T>())`. This is what `OmniAddress::Sui`
/// carries on NEAR and what the MPC-signed `TransferMessagePayload`
/// contains as `token_address`.
public fun token_address_bytes<T>(): vector<u8> {
    hash::keccak256(coin_type_string<T>().as_bytes())
}

/// `token_address_bytes<T>()` as a Sui `address` (for events).
public fun token_address<T>(): address {
    address::from_bytes(token_address_bytes<T>())
}

/// The DEFINING package id of `T` as an address. Used by `deploy_token`
/// to check that a supplied `UpgradeCap` controls `T`'s package.
public fun type_package_address<T>(): address {
    type_name::defining_id<T>()
}
