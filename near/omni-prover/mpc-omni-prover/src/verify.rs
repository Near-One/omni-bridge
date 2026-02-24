use k256::ecdsa::{Signature, VerifyingKey};
use k256::EncodedPoint;

pub fn verify_secp256k1_signature(
    public_key_bytes: &[u8],
    message_hash: &[u8; 32],
    big_r_hex: &str,
    s_hex: &str,
    _recovery_id: u8,
) -> Result<(), String> {
    let encoded_point = EncodedPoint::from_bytes(public_key_bytes)
        .map_err(|e| format!("Invalid public key encoding: {e}"))?;
    let verifying_key = VerifyingKey::from_encoded_point(&encoded_point)
        .map_err(|e| format!("Invalid public key: {e}"))?;

    let big_r_bytes = hex::decode(big_r_hex).map_err(|e| format!("Invalid hex for big_r: {e}"))?;
    if big_r_bytes.len() != 33 {
        return Err(format!(
            "Invalid big_r length: expected 33 bytes, got {}",
            big_r_bytes.len()
        ));
    }
    let r_bytes = &big_r_bytes[1..];

    let s_bytes = hex::decode(s_hex).map_err(|e| format!("Invalid hex for s: {e}"))?;
    if s_bytes.len() != 32 {
        return Err(format!(
            "Invalid s length: expected 32 bytes, got {}",
            s_bytes.len()
        ));
    }

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(r_bytes);
    sig_bytes[32..].copy_from_slice(&s_bytes);

    let signature = Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| format!("Invalid signature: {e}"))?;

    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    verifying_key
        .verify_prehash(message_hash, &signature)
        .map_err(|e| format!("Signature verification failed: {e}"))?;

    Ok(())
}
