use borsh::BorshDeserialize;

use contract_interface::types::{
    EvmExtractedValue, EvmExtractor, EvmFinality, EvmLog, EvmRpcRequest, EvmTxId, ExtractedValue,
    ForeignChainRpcRequest, ForeignTxSignPayload, ForeignTxSignPayloadV1, Hash160, Hash256,
};

use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::ProofKind;

use crate::evm_log_to_rlp;

fn test_evm_log() -> EvmLog {
    EvmLog {
        removed: false,
        log_index: 0,
        transaction_index: 0,
        transaction_hash: Hash256([1u8; 32]),
        block_hash: Hash256([2u8; 32]),
        block_number: 100,
        address: Hash160([3u8; 20]),
        data: "0x".to_string(),
        topics: vec![],
    }
}

fn test_sign_payload() -> ForeignTxSignPayload {
    ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Abstract(EvmRpcRequest {
            tx_id: EvmTxId([0xab; 32]),
            extractors: vec![EvmExtractor::Log { log_index: 0 }],
            finality: EvmFinality::Finalized,
        }),
        values: vec![ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(
            test_evm_log(),
        ))],
    })
}

#[test]
fn test_payload_hash_computation() {
    let payload = test_sign_payload();
    let hash = payload.compute_msg_hash().unwrap();
    assert_eq!(hash.0.len(), 32);

    let hash2 = payload.compute_msg_hash().unwrap();
    assert_eq!(hash.0, hash2.0);
}

#[test]
fn test_payload_serialization_roundtrip() {
    let payload = test_sign_payload();
    let bytes = borsh::to_vec(&payload).unwrap();
    let deserialized = ForeignTxSignPayload::try_from_slice(&bytes).unwrap();

    let original_hash = payload.compute_msg_hash().unwrap();
    let roundtrip_hash = deserialized.compute_msg_hash().unwrap();
    assert_eq!(original_hash.0, roundtrip_hash.0);
}

#[test]
fn test_evm_log_to_rlp_basic() {
    let log = test_evm_log();
    let rlp_bytes = evm_log_to_rlp(&log).unwrap();
    assert!(!rlp_bytes.is_empty());
}

#[test]
fn test_evm_log_to_rlp_with_data() {
    let log = EvmLog {
        removed: false,
        log_index: 5,
        transaction_index: 2,
        transaction_hash: Hash256([0xaa; 32]),
        block_hash: Hash256([0xbb; 32]),
        block_number: 12345,
        address: Hash160([0xcc; 20]),
        data: "0xdeadbeef".to_string(),
        topics: vec![Hash256([0x11; 32]), Hash256([0x22; 32])],
    };

    let rlp_bytes = evm_log_to_rlp(&log).unwrap();
    assert!(!rlp_bytes.is_empty());
}

#[test]
fn test_evm_log_to_rlp_without_0x_prefix() {
    let log = EvmLog {
        removed: false,
        log_index: 0,
        transaction_index: 0,
        transaction_hash: Hash256([0; 32]),
        block_hash: Hash256([0; 32]),
        block_number: 0,
        address: Hash160([0; 20]),
        data: "deadbeef".to_string(),
        topics: vec![],
    };

    let rlp_bytes = evm_log_to_rlp(&log).unwrap();
    assert!(!rlp_bytes.is_empty());
}

#[test]
fn test_mpc_verify_proof_args_serialization() {
    let payload = test_sign_payload();
    let payload_bytes = borsh::to_vec(&payload).unwrap();
    let hash = payload.compute_msg_hash().unwrap();

    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: payload_bytes.clone(),
        payload_hash: hash.0,
        signature_big_r: "02".to_string() + &"ab".repeat(32),
        signature_s: "cd".repeat(32),
        signature_recovery_id: 0,
    };

    let serialized = borsh::to_vec(&args).unwrap();
    let deserialized = MpcVerifyProofArgs::try_from_slice(&serialized).unwrap();

    assert_eq!(deserialized.payload_hash, hash.0);
    assert_eq!(deserialized.sign_payload, payload_bytes);
}

#[test]
fn test_verify_signature_with_known_keypair() {
    use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};

    let signing_key = SigningKey::from_bytes(&[1u8; 32].into()).unwrap();
    let verifying_key = signing_key.verifying_key();
    let public_key_bytes = verifying_key.to_encoded_point(true);

    let payload = test_sign_payload();
    let hash = payload.compute_msg_hash().unwrap();

    let (signature, recovery_id) = signing_key
        .sign_prehash(&hash.0)
        .expect("signing should succeed");

    let sig_bytes = signature.to_bytes();
    let r_bytes = &sig_bytes[..32];
    let s_bytes = &sig_bytes[32..];

    let big_r_hex = {
        let mut big_r = vec![0x02u8];
        big_r.extend_from_slice(r_bytes);
        hex::encode(&big_r)
    };

    let s_hex = hex::encode(s_bytes);

    let result = crate::verify::verify_secp256k1_signature(
        public_key_bytes.as_bytes(),
        &hash.0,
        &big_r_hex,
        &s_hex,
        recovery_id.to_byte(),
    );

    assert!(
        result.is_ok(),
        "Signature verification failed: {:?}",
        result.err()
    );
}

#[test]
fn test_verify_signature_wrong_key_fails() {
    use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};

    let signing_key = SigningKey::from_bytes(&[1u8; 32].into()).unwrap();
    let wrong_key = SigningKey::from_bytes(&[2u8; 32].into()).unwrap();
    let wrong_public_key = wrong_key.verifying_key().to_encoded_point(true);

    let payload = test_sign_payload();
    let hash = payload.compute_msg_hash().unwrap();

    let (signature, recovery_id) = signing_key
        .sign_prehash(&hash.0)
        .expect("signing should succeed");

    let sig_bytes = signature.to_bytes();
    let r_bytes = &sig_bytes[..32];
    let s_bytes = &sig_bytes[32..];

    let mut big_r = vec![0x02u8];
    big_r.extend_from_slice(r_bytes);

    let result = crate::verify::verify_secp256k1_signature(
        wrong_public_key.as_bytes(),
        &hash.0,
        &hex::encode(&big_r),
        &hex::encode(s_bytes),
        recovery_id.to_byte(),
    );

    assert!(result.is_err(), "Verification should fail with wrong key");
}

#[test]
fn test_verify_signature_wrong_hash_fails() {
    use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};

    let signing_key = SigningKey::from_bytes(&[1u8; 32].into()).unwrap();
    let public_key = signing_key.verifying_key().to_encoded_point(true);

    let payload = test_sign_payload();
    let hash = payload.compute_msg_hash().unwrap();

    let (signature, recovery_id) = signing_key
        .sign_prehash(&hash.0)
        .expect("signing should succeed");

    let sig_bytes = signature.to_bytes();
    let r_bytes = &sig_bytes[..32];
    let s_bytes = &sig_bytes[32..];

    let mut big_r = vec![0x02u8];
    big_r.extend_from_slice(r_bytes);

    let wrong_hash = [0xffu8; 32];

    let result = crate::verify::verify_secp256k1_signature(
        public_key.as_bytes(),
        &wrong_hash,
        &hex::encode(&big_r),
        &hex::encode(s_bytes),
        recovery_id.to_byte(),
    );

    assert!(result.is_err(), "Verification should fail with wrong hash");
}
