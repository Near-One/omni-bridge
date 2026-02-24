use k256::ecdsa::{Signature, VerifyingKey};
use k256::EncodedPoint;
use near_sdk::require;
use omni_types::errors::ProverError;
use omni_utils::near_expect::NearExpect;

pub fn verify_secp256k1_signature(
    public_key_bytes: &[u8],
    message_hash: &[u8; 32],
    big_r_hex: &str,
    s_hex: &str,
    _recovery_id: u8,
) -> Result<(), String> {
    let encoded_point =
        EncodedPoint::from_bytes(public_key_bytes).near_expect(ProverError::InvalidPublicKey);
    let verifying_key =
        VerifyingKey::from_encoded_point(&encoded_point).near_expect(ProverError::InvalidPublicKey);

    let big_r_bytes = hex::decode(big_r_hex).near_expect(ProverError::InvalidSignature);
    require!(
        big_r_bytes.len() == 33,
        ProverError::InvalidSignature.as_ref()
    );

    let r_bytes = &big_r_bytes[1..];

    let s_bytes = hex::decode(s_hex).near_expect(ProverError::InvalidSignature);
    require!(s_bytes.len() == 32, ProverError::InvalidSignature.as_ref());

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(r_bytes);
    sig_bytes[32..].copy_from_slice(&s_bytes);

    let signature =
        Signature::from_bytes((&sig_bytes).into()).near_expect(ProverError::InvalidSignature);

    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    verifying_key
        .verify_prehash(message_hash, &signature)
        .near_expect(ProverError::InvalidSignature);

    Ok(())
}
