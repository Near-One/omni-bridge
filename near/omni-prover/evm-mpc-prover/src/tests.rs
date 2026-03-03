use borsh::BorshDeserialize;

use near_mpc_sdk::{
    contract_interface::types::{
        DomainId, EvmExtractedValue, EvmExtractor, EvmFinality, EvmLog, EvmRpcRequest, EvmTxId,
        ExtractedValue, ForeignChainRpcRequest, ForeignTxSignPayload, ForeignTxSignPayloadV1,
        Hash160, Hash256, SolanaFinality, SolanaRpcRequest, SolanaTxId,
    },
};

use near_sdk::base64::Engine;
use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::ProofKind;

use omni_types::ChainKind;

use crate::{evm_log_to_rlp, EvmMpcProver};

fn hex_to_hash256(hex_str: &str) -> Hash256 {
    let bytes: [u8; 32] = hex::decode(hex_str).unwrap().try_into().unwrap();
    Hash256(bytes)
}

fn hex_to_hash160(hex_str: &str) -> Hash160 {
    let bytes: [u8; 20] = hex::decode(hex_str).unwrap().try_into().unwrap();
    Hash160(bytes)
}

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

/// Real InitTransfer log from Abstract testnet tx:
/// https://sepolia.abscan.org/tx/0x8d286a01fa892903128228cdca896de68c7b774ccbd1b46b25867ef9499c6fc3
/// Log index 3 — InitTransfer event from bridge contract 0x1a7Eba78B12F2A82D812f25155e6c7FC2aB1eD32
/// initTransfer(tokenAddress=0x6641415a..., amount=1, fee=0, nativeFee=0, recipient="near:frolik.testnet")
fn abs_testnet_evm_log() -> EvmLog {
    EvmLog {
        removed: false,
        log_index: 3,
        transaction_index: 0,
        transaction_hash: hex_to_hash256(
            "8d286a01fa892903128228cdca896de68c7b774ccbd1b46b25867ef9499c6fc3",
        ),
        block_hash: hex_to_hash256(
            "45471ca1210369f7e2062ea93a4a173c4573460497f01303bc3b6c8b6a84dec1",
        ),
        block_number: 0xfee757,
        address: hex_to_hash160("1a7eba78b12f2a82d812f25155e6c7fc2ab1ed32"),
        data: "0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000000136e6561723a66726f6c696b2e746573746e6574000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        topics: vec![
            hex_to_hash256(
                "aa7e1f77d43faa300bc5ae8f012f0b7cf80174f4c0b1cffeab250cb4966bb88c",
            ),
            hex_to_hash256(
                "000000000000000000000000cf6462b9fce5af3e6c660c83453eca18ff468773",
            ),
            hex_to_hash256(
                "0000000000000000000000006641415a61bce80d97a715054d1334360ab833eb",
            ),
            hex_to_hash256(
                "0000000000000000000000000000000000000000000000000000000000000001",
            ),
        ],
    }
}

fn abs_testnet_tx_id() -> EvmTxId {
    let bytes: [u8; 32] =
        hex::decode("8d286a01fa892903128228cdca896de68c7b774ccbd1b46b25867ef9499c6fc3")
            .unwrap()
            .try_into()
            .unwrap();
    EvmTxId(bytes)
}

fn test_evm_request() -> EvmRpcRequest {
    EvmRpcRequest {
        tx_id: EvmTxId([0xab; 32]),
        extractors: vec![EvmExtractor::Log { log_index: 0 }],
        finality: EvmFinality::Finalized,
    }
}

fn abs_testnet_evm_request() -> EvmRpcRequest {
    EvmRpcRequest {
        tx_id: abs_testnet_tx_id(),
        extractors: vec![EvmExtractor::Log { log_index: 3 }],
        finality: EvmFinality::Latest,
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
        derivation_path: "".to_string(),
        domain_id: DomainId(3),
        payload_version: 1,
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

    assert!(EvmMpcProver::request_matches_chain(
        &eth_request,
        ChainKind::Eth
    ));
    assert!(!EvmMpcProver::request_matches_chain(
        &eth_request,
        ChainKind::Base
    ));
}

#[test]
fn test_request_matches_chain_abstract() {
    let abs_request = ForeignChainRpcRequest::Abstract(test_evm_request());

    // Abstract request only matches Abs
    assert!(EvmMpcProver::request_matches_chain(
        &abs_request,
        ChainKind::Abs
    ));

    // Abstract request does NOT match other EVM chains
    assert!(!EvmMpcProver::request_matches_chain(
        &abs_request,
        ChainKind::Eth
    ));
    assert!(!EvmMpcProver::request_matches_chain(
        &abs_request,
        ChainKind::Base
    ));
    assert!(!EvmMpcProver::request_matches_chain(
        &abs_request,
        ChainKind::Arb
    ));
}

#[test]
fn test_abs_testnet_verify_proof_args() {
    let request = abs_testnet_evm_request();

    let sign_payload = ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Abstract(request),
        values: vec![ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(
            abs_testnet_evm_log(),
        ))],
    });

    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: borsh::to_vec(&sign_payload).unwrap(),
        derivation_path: String::new(),
        domain_id: DomainId(3),
        payload_version: 1,
    };

    // Verify serialization roundtrip
    let inner_bytes = borsh::to_vec(&args).unwrap();
    let deserialized = MpcVerifyProofArgs::try_from_slice(&inner_bytes).unwrap();
    assert_eq!(deserialized.proof_kind, ProofKind::InitTransfer);

    // Verify payload hash is deterministic
    let payload_from_args =
        ForeignTxSignPayload::try_from_slice(&deserialized.sign_payload).unwrap();
    let hash1 = sign_payload.compute_msg_hash().unwrap();
    let hash2 = payload_from_args.compute_msg_hash().unwrap();
    assert_eq!(hash1.0, hash2.0);

    // Generate base64 for near-cli call (wrapped as borsh Vec<u8> for verify_proof input)
    let call_bytes = borsh::to_vec(&inner_bytes).unwrap();
    let base64_encoded = near_sdk::base64::engine::general_purpose::STANDARD.encode(&call_bytes);

    // Print for manual on-chain testing:
    // near contract call-function as-transaction <prover> verify_proof \
    //   base64-args '<base64_encoded>' prepaid-gas '100 Tgas' ...
    assert!(!base64_encoded.is_empty());
}

#[test]
fn test_request_matches_finality_ethereum_match() {
    let request = ForeignChainRpcRequest::Ethereum(test_evm_request()); // Finalized
    assert!(EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Finalized
    ));
}

#[test]
fn test_request_matches_finality_abstract_match() {
    let request = ForeignChainRpcRequest::Abstract(abs_testnet_evm_request()); // Latest
    assert!(EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Latest
    ));
}

#[test]
fn test_request_matches_finality_ethereum_mismatch() {
    let request = ForeignChainRpcRequest::Ethereum(test_evm_request()); // Finalized
    assert!(!EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Latest
    ));
    assert!(!EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Safe
    ));
}

#[test]
fn test_request_matches_finality_abstract_mismatch() {
    let request = ForeignChainRpcRequest::Abstract(abs_testnet_evm_request()); // Latest
    assert!(!EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Finalized
    ));
    assert!(!EvmMpcProver::request_matches_finality(
        &request,
        &EvmFinality::Safe
    ));
}

#[test]
fn test_request_matches_finality_non_evm_returns_false() {
    let solana_request = ForeignChainRpcRequest::Solana(SolanaRpcRequest {
        tx_id: SolanaTxId([0u8; 64]),
        finality: SolanaFinality::Confirmed,
        extractors: vec![],
    });

    // Non-EVM requests should never match any EVM finality
    assert!(!EvmMpcProver::request_matches_finality(
        &solana_request,
        &EvmFinality::Latest
    ));
    assert!(!EvmMpcProver::request_matches_finality(
        &solana_request,
        &EvmFinality::Safe
    ));
    assert!(!EvmMpcProver::request_matches_finality(
        &solana_request,
        &EvmFinality::Finalized
    ));
}
