/// Shared utilities for the Omni Bridge: Ethereum-style signature
/// verification (used to validate MPC-signed payloads from NEAR) and
/// a decimal normalization helper.
module omni_bridge::utils {
    use aptos_std::aptos_hash;
    use aptos_std::secp256k1;

    /// Signature payload could not be parsed.
    const E_INVALID_SIGNATURE_LENGTH: u64 = 1;
    /// `ecdsa_recover` failed to recover a public key.
    const E_RECOVER_FAILED: u64 = 2;
    /// Recovered Ethereum address does not match the expected signer.
    const E_INVALID_SIGNATURE: u64 = 3;

    const MAX_ALLOWED_DECIMALS: u8 = 18;

    /// Cap decimals at the protocol-wide maximum.
    public fun normalize_decimals(decimals: u8): u8 {
        if (decimals > MAX_ALLOWED_DECIMALS) {
            MAX_ALLOWED_DECIMALS
        } else {
            decimals
        }
    }

    /// Verify an Ethereum-style signature (r || s, v) over `message_bytes`.
    ///
    /// Computes `keccak256(message_bytes)`, recovers the secp256k1 public
    /// key, derives the Ethereum address (last 20 bytes of `keccak256(pk)`),
    /// and asserts equality with `expected_address`.
    ///
    /// `v` is the Ethereum recovery id: 27, 28, or 0/1. Internally
    /// normalized to {0,1} for `secp256k1::ecdsa_recover`.
    public fun verify_eth_signature(
        message_bytes: vector<u8>,
        signature_rs: vector<u8>,
        v: u8,
        expected_address: vector<u8>,
    ) {
        assert!(signature_rs.length() == 64, E_INVALID_SIGNATURE_LENGTH);
        assert!(expected_address.length() == 20, E_INVALID_SIGNATURE);

        let message_hash = aptos_hash::keccak256(message_bytes);

        let recovery_id = if (v >= 27) { v - 27 } else { v };

        let sig = secp256k1::ecdsa_signature_from_bytes(signature_rs);
        let recovered = secp256k1::ecdsa_recover(message_hash, recovery_id, &sig);
        assert!(recovered.is_some(), E_RECOVER_FAILED);
        let pk = recovered.extract();
        let pk_bytes = secp256k1::ecdsa_raw_public_key_to_bytes(&pk);

        let pk_hash = aptos_hash::keccak256(pk_bytes);
        let addr = last_20_bytes(&pk_hash);

        assert!(addr == expected_address, E_INVALID_SIGNATURE);
    }

    /// Return the last 20 bytes of a 32-byte digest. Aborts on shorter input.
    fun last_20_bytes(digest: &vector<u8>): vector<u8> {
        let len = digest.length();
        assert!(len >= 20, E_INVALID_SIGNATURE);
        let result = vector[];
        for (i in (len - 20)..len) {
            result.push_back(digest[i]);
        };
        result
    }

    #[test_only]
    public fun test_normalize_decimals(d: u8): u8 { normalize_decimals(d) }

    #[test_only]
    public fun test_verify_eth_signature(
        message_bytes: vector<u8>,
        signature_rs: vector<u8>,
        v: u8,
        expected_address: vector<u8>,
    ) {
        verify_eth_signature(message_bytes, signature_rs, v, expected_address);
    }
}
