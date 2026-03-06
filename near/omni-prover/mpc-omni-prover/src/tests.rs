use borsh::BorshDeserialize;

use near_mpc_sdk::contract_interface::types::{
    EvmExtractedValue, EvmExtractor, EvmFinality, EvmLog, EvmRpcRequest, EvmTxId, ExtractedValue,
    ForeignChainRpcRequest, ForeignTxSignPayload, ForeignTxSignPayloadV1, Hash160, Hash256,
    SolanaFinality, SolanaRpcRequest, SolanaTxId, StarknetExtractedValue, StarknetExtractor,
    StarknetFelt, StarknetFinality, StarknetLog, StarknetRpcRequest, StarknetTxId,
};

use near_sdk::base64::Engine;
use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::ProofKind;

use omni_types::ChainKind;

use crate::{evm_log_to_rlp, MpcFinality, MpcOmniProver};

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

fn test_starknet_request() -> StarknetRpcRequest {
    StarknetRpcRequest {
        tx_id: StarknetTxId(StarknetFelt([0xcc; 32])),
        finality: StarknetFinality::AcceptedOnL1,
        extractors: vec![StarknetExtractor::Log { log_index: 0 }],
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
fn test_request_to_chain_kind_ethereum() {
    let request = ForeignChainRpcRequest::Ethereum(test_evm_request());
    assert_eq!(
        MpcOmniProver::request_to_chain_kind(&request),
        Some(ChainKind::Eth)
    );
}

#[test]
fn test_request_to_chain_kind_abstract() {
    let request = ForeignChainRpcRequest::Abstract(test_evm_request());
    assert_eq!(
        MpcOmniProver::request_to_chain_kind(&request),
        Some(ChainKind::Abs)
    );
}

#[test]
fn test_request_to_chain_kind_starknet() {
    let request = ForeignChainRpcRequest::Starknet(test_starknet_request());
    assert_eq!(
        MpcOmniProver::request_to_chain_kind(&request),
        Some(ChainKind::Strk)
    );
}

#[test]
fn test_request_to_chain_kind_unsupported() {
    let solana_request = ForeignChainRpcRequest::Solana(SolanaRpcRequest {
        tx_id: SolanaTxId([0u8; 64]),
        finality: SolanaFinality::Confirmed,
        extractors: vec![],
    });
    assert_eq!(MpcOmniProver::request_to_chain_kind(&solana_request), None);
}

#[test]
fn test_request_matches_finality_ethereum_match() {
    let request = ForeignChainRpcRequest::Ethereum(test_evm_request());
    assert!(MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Finalized)
    ));
}

#[test]
fn test_request_matches_finality_abstract_match() {
    let request = ForeignChainRpcRequest::Abstract(abs_testnet_evm_request());
    assert!(MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Latest)
    ));
}

#[test]
fn test_request_matches_finality_ethereum_mismatch() {
    let request = ForeignChainRpcRequest::Ethereum(test_evm_request());
    assert!(!MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Latest)
    ));
    assert!(!MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Safe)
    ));
}

#[test]
fn test_request_matches_finality_abstract_mismatch() {
    let request = ForeignChainRpcRequest::Abstract(abs_testnet_evm_request());
    assert!(!MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Finalized)
    ));
    assert!(!MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Evm(EvmFinality::Safe)
    ));
}

#[test]
fn test_request_matches_finality_starknet_match() {
    let request = ForeignChainRpcRequest::Starknet(test_starknet_request());
    assert!(MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Starknet(StarknetFinality::AcceptedOnL1)
    ));
}

#[test]
fn test_request_matches_finality_starknet_mismatch() {
    let request = ForeignChainRpcRequest::Starknet(test_starknet_request());
    assert!(!MpcOmniProver::request_matches_finality(
        &request,
        &MpcFinality::Starknet(StarknetFinality::AcceptedOnL2)
    ));
}

#[test]
fn test_request_matches_finality_cross_chain_mismatch() {
    let evm_request = ForeignChainRpcRequest::Ethereum(test_evm_request());
    assert!(!MpcOmniProver::request_matches_finality(
        &evm_request,
        &MpcFinality::Starknet(StarknetFinality::AcceptedOnL1)
    ));

    let strk_request = ForeignChainRpcRequest::Starknet(test_starknet_request());
    assert!(!MpcOmniProver::request_matches_finality(
        &strk_request,
        &MpcFinality::Evm(EvmFinality::Finalized)
    ));
}

#[test]
fn test_request_matches_finality_non_evm_returns_false() {
    let solana_request = ForeignChainRpcRequest::Solana(SolanaRpcRequest {
        tx_id: SolanaTxId([0u8; 64]),
        finality: SolanaFinality::Confirmed,
        extractors: vec![],
    });

    assert!(!MpcOmniProver::request_matches_finality(
        &solana_request,
        &MpcFinality::Evm(EvmFinality::Latest)
    ));
    assert!(!MpcOmniProver::request_matches_finality(
        &solana_request,
        &MpcFinality::Starknet(StarknetFinality::AcceptedOnL1)
    ));
}

#[test]
fn test_starknet_sign_payload_roundtrip() {
    let payload = ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Starknet(test_starknet_request()),
        values: vec![ExtractedValue::StarknetExtractedValue(
            StarknetExtractedValue::Log(StarknetLog {
                block_hash: StarknetFelt([0x01; 32]),
                block_number: 100,
                data: vec![StarknetFelt([0x02; 32])],
                from_address: StarknetFelt([0x03; 32]),
                keys: vec![StarknetFelt([0x04; 32])],
            }),
        )],
    });

    let bytes = borsh::to_vec(&payload).unwrap();
    let deserialized = ForeignTxSignPayload::try_from_slice(&bytes).unwrap();

    let hash1 = payload.compute_msg_hash().unwrap();
    let hash2 = deserialized.compute_msg_hash().unwrap();
    assert_eq!(hash1.0, hash2.0);
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

    let call_bytes = borsh::to_vec(&inner_bytes).unwrap();
    let base64_encoded = near_sdk::base64::engine::general_purpose::STANDARD.encode(&call_bytes);

    assert!(!base64_encoded.is_empty());
}

fn hex_to_starknet_felt(hex_str: &str) -> StarknetFelt {
    let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let padded = if stripped.len() % 2 == 1 {
        format!("0{stripped}")
    } else {
        stripped.to_string()
    };
    let bytes = hex::decode(&padded).unwrap();
    let mut felt = [0u8; 32];
    felt[32 - bytes.len()..].copy_from_slice(&bytes);
    StarknetFelt(felt)
}

/// Real InitTransfer event from Starknet sepolia tx:
/// https://sepolia.starkscan.co/tx/0x0592d937f74565b8c42c5603083e5536fdcd8e585b5fef5cd5c2c04b65cd80e5
/// Event index 3 from bridge contract `0x05a0ad01b...98bdc8f`
/// `initTransfer`(token=STRK, sender=`0x01bc36c7a...`, amount=10, recipient="near:frolik.testnet")
fn starknet_sepolia_log() -> StarknetLog {
    StarknetLog {
        block_hash: hex_to_starknet_felt(
            "0x05eed39a16c3695b707b24ee75c43c0dc0ce94d07d73f39473d6871a60fd07a7",
        ),
        block_number: 6_389_548,
        from_address: hex_to_starknet_felt(
            "0x05a0ad01b18eba34432d22e4cb5c987560cae87a785b494ed58d9553a98bdc8f",
        ),
        keys: vec![
            hex_to_starknet_felt(
                "0xdf95b9d93d8073acda8a048cb25360af6f665c6dfd33d86af06c20b4573c75",
            ),
            hex_to_starknet_felt(
                "0x1bc36c7a215b86ea1d8943d2addbfdd767d5d8ff1258cb03fc40f6d69e6008",
            ),
            hex_to_starknet_felt(
                "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
            ),
            hex_to_starknet_felt("0x1"),
        ],
        data: vec![
            hex_to_starknet_felt("0x0a"),
            hex_to_starknet_felt("0x0"),
            hex_to_starknet_felt("0x0"),
            hex_to_starknet_felt("0x0"),
            hex_to_starknet_felt("0x6e6561723a66726f6c696b2e746573746e6574"),
            hex_to_starknet_felt("0x13"),
            hex_to_starknet_felt("0x0"),
            hex_to_starknet_felt("0x0"),
            hex_to_starknet_felt("0x0"),
        ],
    }
}

fn starknet_sepolia_request() -> StarknetRpcRequest {
    StarknetRpcRequest {
        tx_id: StarknetTxId(hex_to_starknet_felt(
            "0x0592d937f74565b8c42c5603083e5536fdcd8e585b5fef5cd5c2c04b65cd80e5",
        )),
        finality: StarknetFinality::AcceptedOnL2,
        extractors: vec![StarknetExtractor::Log { log_index: 3 }],
    }
}

#[test]
fn test_starknet_sepolia_verify_proof_args() {
    let request = starknet_sepolia_request();

    let starknet_log = starknet_sepolia_log();
    let sign_payload = ForeignTxSignPayload::V1(ForeignTxSignPayloadV1 {
        request: ForeignChainRpcRequest::Starknet(request),
        values: vec![ExtractedValue::StarknetExtractedValue(
            StarknetExtractedValue::Log(starknet_log),
        )],
    });

    let args = MpcVerifyProofArgs {
        proof_kind: ProofKind::InitTransfer,
        sign_payload: borsh::to_vec(&sign_payload).unwrap(),
    };

    let inner_bytes = borsh::to_vec(&args).unwrap();
    let deserialized = MpcVerifyProofArgs::try_from_slice(&inner_bytes).unwrap();
    assert_eq!(deserialized.proof_kind, ProofKind::InitTransfer);

    let payload_from_args =
        ForeignTxSignPayload::try_from_slice(&deserialized.sign_payload).unwrap();
    let hash1 = sign_payload.compute_msg_hash().unwrap();
    let hash2 = payload_from_args.compute_msg_hash().unwrap();
    assert_eq!(hash1.0, hash2.0);

    let call_bytes = borsh::to_vec(&inner_bytes).unwrap();
    let base64_encoded = near_sdk::base64::engine::general_purpose::STANDARD.encode(&call_bytes);

    assert!(!base64_encoded.is_empty());
}
