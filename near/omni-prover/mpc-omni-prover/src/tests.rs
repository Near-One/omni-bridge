use borsh::BorshDeserialize;

use near_mpc_sdk::contract_interface::types::{
    EvmExtractedValue, EvmExtractor, EvmFinality, EvmLog, EvmRpcRequest, EvmTxId, ExtractedValue,
    ForeignChainRpcRequest, ForeignTxSignPayload, ForeignTxSignPayloadV1, Hash160, Hash256,
};

use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::ProofKind;

use omni_types::ChainKind;

use crate::{evm_log_to_rlp, MpcOmniProver};

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

fn test_evm_request() -> EvmRpcRequest {
    EvmRpcRequest {
        tx_id: EvmTxId([0xab; 32]),
        extractors: vec![EvmExtractor::Log { log_index: 0 }],
        finality: EvmFinality::Finalized,
    }
}

fn test_sign_payload() -> ForeignTxSignPayload {
    ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Abstract(test_evm_request()),
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

    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: payload_bytes.clone(),
        request_args_json: r#"{"request":{"Abstract":{"tx_id":"abababababababababababababababababababababababababababababababababab","extractors":[{"Log":{"log_index":0}}],"finality":"Finalized"}},"derivation_path":"","domain_id":0,"payload_version":1}"#.to_string(),
    };

    let serialized = borsh::to_vec(&args).unwrap();
    let deserialized = MpcVerifyProofArgs::try_from_slice(&serialized).unwrap();

    assert_eq!(deserialized.sign_payload, payload_bytes);
    assert_eq!(deserialized.proof_kind, ProofKind::InitTransfer);
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
fn test_forged_payload_produces_different_hash() {
    let original_payload = test_sign_payload();
    let original_hash = original_payload.compute_msg_hash().unwrap();

    let forged_payload = ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Ethereum(test_evm_request()),
        values: vec![ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(
            test_evm_log(),
        ))],
    });
    let forged_hash = forged_payload.compute_msg_hash().unwrap();

    assert_ne!(
        original_hash.0, forged_hash.0,
        "Different chain requests must produce different hashes"
    );
}

#[test]
fn test_request_matches_chain_ethereum_variants() {
    let eth_request = ForeignChainRpcRequest::Ethereum(test_evm_request());

    assert!(MpcOmniProver::request_matches_chain(
        &eth_request,
        ChainKind::Eth
    ));
    assert!(!MpcOmniProver::request_matches_chain(
        &eth_request,
        ChainKind::Base
    ));
}

#[test]
fn test_request_matches_chain_abstract() {
    let abs_request = ForeignChainRpcRequest::Abstract(test_evm_request());

    // Abstract request only matches Abs
    assert!(MpcOmniProver::request_matches_chain(
        &abs_request,
        ChainKind::Abs
    ));

    // Abstract request does NOT match other EVM chains
    assert!(!MpcOmniProver::request_matches_chain(
        &abs_request,
        ChainKind::Eth
    ));
    assert!(!MpcOmniProver::request_matches_chain(
        &abs_request,
        ChainKind::Base
    ));
    assert!(!MpcOmniProver::request_matches_chain(
        &abs_request,
        ChainKind::Arb
    ));
}
