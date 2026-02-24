use borsh::BorshDeserialize;

use contract_interface::types::{
    EvmExtractedValue, EvmExtractor, EvmFinality, EvmLog, EvmRpcRequest, EvmTxId, ExtractedValue,
    ForeignChainRpcRequest, ForeignTxSignPayload, ForeignTxSignPayloadV1, Hash160, Hash256,
};

use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::ProofKind;
use omni_types::ChainKind;

use crate::{chain_kind_to_foreign_chain, evm_log_to_rlp};

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
    assert!(
        result.unwrap_err().contains("ERR_INVALID_SIGNATURE"),
        "Error should be ERR_INVALID_SIGNATURE"
    );
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
    assert!(
        result.unwrap_err().contains("ERR_INVALID_SIGNATURE"),
        "Error should be ERR_INVALID_SIGNATURE"
    );
}

#[test]
fn test_chain_kind_to_foreign_chain_mapping() {
    use contract_interface::types::ForeignChain;

    assert_eq!(
        chain_kind_to_foreign_chain(ChainKind::Abs),
        Some(ForeignChain::Abstract)
    );
    assert_eq!(
        chain_kind_to_foreign_chain(ChainKind::Eth),
        Some(ForeignChain::Ethereum)
    );
    assert_eq!(
        chain_kind_to_foreign_chain(ChainKind::Arb),
        Some(ForeignChain::Arbitrum)
    );
    assert_eq!(
        chain_kind_to_foreign_chain(ChainKind::Base),
        Some(ForeignChain::Base)
    );
    assert_eq!(
        chain_kind_to_foreign_chain(ChainKind::Bnb),
        Some(ForeignChain::Bnb)
    );
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Near), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Sol), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Btc), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Strk), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Zcash), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::Pol), None);
    assert_eq!(chain_kind_to_foreign_chain(ChainKind::HyperEvm), None);
}

#[test]
fn test_chain_kind_validation_matching() {
    let payload = test_sign_payload();
    let ForeignTxSignPayload::V1(ref v1) = payload;
    let payload_chain = v1.request.chain();
    let expected = chain_kind_to_foreign_chain(ChainKind::Abs).unwrap();
    assert_eq!(payload_chain, expected);
}

#[test]
fn test_chain_kind_validation_mismatch_detected() {
    let payload = test_sign_payload();
    let ForeignTxSignPayload::V1(ref v1) = payload;
    let payload_chain = v1.request.chain();

    let wrong_expected = chain_kind_to_foreign_chain(ChainKind::Eth).unwrap();
    assert_ne!(
        payload_chain, wrong_expected,
        "Abstract payload should not match Ethereum chain"
    );
}

fn make_ethereum_sign_payload() -> ForeignTxSignPayload {
    ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Ethereum(EvmRpcRequest {
            tx_id: EvmTxId([0xee; 32]),
            extractors: vec![EvmExtractor::Log { log_index: 0 }],
            finality: EvmFinality::Finalized,
        }),
        values: vec![ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(
            test_evm_log(),
        ))],
    })
}

#[test]
fn test_chain_kind_validation_ethereum_payload_against_abs_prover() {
    let payload = make_ethereum_sign_payload();
    let ForeignTxSignPayload::V1(ref v1) = payload;
    let payload_chain = v1.request.chain();

    let abs_expected = chain_kind_to_foreign_chain(ChainKind::Abs).unwrap();
    assert_ne!(
        payload_chain, abs_expected,
        "Ethereum payload must be rejected by Abstract prover"
    );

    let eth_expected = chain_kind_to_foreign_chain(ChainKind::Eth).unwrap();
    assert_eq!(
        payload_chain, eth_expected,
        "Ethereum payload should match Ethereum chain"
    );
}

#[test]
fn test_payload_hash_mismatch_detected() {
    let payload = test_sign_payload();
    let computed_hash = payload.compute_msg_hash().unwrap();
    let wrong_hash = [0xffu8; 32];
    assert_ne!(
        computed_hash.0, wrong_hash,
        "Computed hash must differ from forged hash"
    );
}

#[test]
fn test_full_verify_pipeline_valid() {
    use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};

    let signing_key = SigningKey::from_bytes(&[1u8; 32].into()).unwrap();
    let public_key_bytes = signing_key.verifying_key().to_encoded_point(true);

    let payload = test_sign_payload();
    let payload_bytes = borsh::to_vec(&payload).unwrap();
    let computed_hash = payload.compute_msg_hash().unwrap();

    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: payload_bytes.clone(),
        payload_hash: computed_hash.0,
        signature_big_r: String::new(),
        signature_s: String::new(),
        signature_recovery_id: 0,
    };

    // Step 1: Deserialize and recompute hash
    let deserialized_payload = ForeignTxSignPayload::try_from_slice(&args.sign_payload).unwrap();
    let recomputed = deserialized_payload.compute_msg_hash().unwrap();
    assert_eq!(recomputed.0, args.payload_hash, "Hash must match");

    // Step 2: Sign with the recomputed hash and verify signature
    let (signature, recovery_id) = signing_key.sign_prehash(&recomputed.0).unwrap();
    let sig_bytes = signature.to_bytes();
    let mut big_r = vec![0x02u8];
    big_r.extend_from_slice(&sig_bytes[..32]);

    let result = crate::verify::verify_secp256k1_signature(
        public_key_bytes.as_bytes(),
        &recomputed.0,
        &hex::encode(&big_r),
        &hex::encode(&sig_bytes[32..]),
        recovery_id.to_byte(),
    );
    assert!(result.is_ok(), "Valid signature must pass");

    // Step 3: Chain validation
    let ForeignTxSignPayload::V1(ref v1) = deserialized_payload;
    let payload_chain = v1.request.chain();
    let expected = chain_kind_to_foreign_chain(ChainKind::Abs).unwrap();
    assert_eq!(payload_chain, expected, "Chain must match");

    // Step 4: EVM log extraction
    let log_data = crate::MpcOmniProver::extract_evm_log(v1);
    assert!(log_data.is_ok(), "EVM log extraction must succeed");
}

#[test]
fn test_full_verify_pipeline_forged_payload_rejected() {
    use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey};

    let signing_key = SigningKey::from_bytes(&[1u8; 32].into()).unwrap();

    // Sign the original payload
    let original_payload = test_sign_payload();
    let original_hash = original_payload.compute_msg_hash().unwrap();
    let (signature, recovery_id) = signing_key.sign_prehash(&original_hash.0).unwrap();
    let sig_bytes = signature.to_bytes();
    let mut big_r = vec![0x02u8];
    big_r.extend_from_slice(&sig_bytes[..32]);

    // Create a different payload
    let forged_payload = make_ethereum_sign_payload();
    let forged_bytes = borsh::to_vec(&forged_payload).unwrap();
    let forged_hash = forged_payload.compute_msg_hash().unwrap();

    // Forged args: different sign_payload but original payload_hash
    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: forged_bytes,
        payload_hash: original_hash.0,
        signature_big_r: hex::encode(&big_r),
        signature_s: hex::encode(&sig_bytes[32..]),
        signature_recovery_id: recovery_id.to_byte(),
    };

    // Recompute hash from the forged payload — it won't match
    let deserialized = ForeignTxSignPayload::try_from_slice(&args.sign_payload).unwrap();
    let recomputed = deserialized.compute_msg_hash().unwrap();
    assert_ne!(
        recomputed.0, args.payload_hash,
        "Forged payload hash must differ — this is the check that prevents the attack"
    );

    // Also confirm the forged hash differs from original
    assert_ne!(forged_hash.0, original_hash.0);
}
