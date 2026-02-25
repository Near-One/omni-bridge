use borsh::BorshDeserialize;
use near_sdk::{near, require, PanicOnDefault};

use near_mpc_sdk::contract_interface::types::{
    DomainId, EvmExtractedValue, EvmRpcRequest, ExtractedValue, ForeignChainRpcRequest,
    ForeignTxSignPayload, ForeignTxSignPayloadV1, PublicKey, VerifyForeignTransactionResponse,
};
use near_mpc_sdk::foreign_chain::ForeignChainRequestBuilder;

use omni_types::errors::ProverError;
use omni_types::evm::events::parse_evm_event;
use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};
use omni_types::ChainKind;
use omni_utils::near_expect::NearExpect;

#[cfg(test)]
mod tests;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MpcOmniProver {
    pub mpc_public_key: String,
    pub chain_kind: ChainKind,
}

#[near]
impl MpcOmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(mpc_public_key: String, chain_kind: ChainKind) -> Self {
        require!(
            chain_kind.is_evm_chain(),
            ProverError::UnsupportedChain.as_ref()
        );

        mpc_public_key
            .parse::<PublicKey>()
            .near_expect(ProverError::InvalidPublicKey);

        Self {
            mpc_public_key,
            chain_kind,
        }
    }

    #[private]
    pub fn update_mpc_public_key(&mut self, mpc_public_key: String) {
        mpc_public_key
            .parse::<PublicKey>()
            .near_expect(ProverError::InvalidPublicKey);

        self.mpc_public_key = mpc_public_key;
    }

    #[allow(clippy::needless_pass_by_value)]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof(
        &self,
        #[serializer(borsh)] input: Vec<u8>,
    ) -> Result<ProverResult, String> {
        let args = MpcVerifyProofArgs::try_from_slice(&input).near_expect(ProverError::ParseArgs);

        let sign_payload = ForeignTxSignPayload::try_from_slice(&args.sign_payload)
            .near_expect(ProverError::ParseArgs);

        let mpc_response: VerifyForeignTransactionResponse =
            serde_json::from_str(&args.mpc_response_json).near_expect(ProverError::ParseArgs);

        let public_key: PublicKey = self
            .mpc_public_key
            .parse()
            .near_expect(ProverError::InvalidPublicKey);

        let ForeignTxSignPayload::V1(ref payload_v1) = sign_payload;

        let (ForeignChainRpcRequest::Ethereum(evm_request)
        | ForeignChainRpcRequest::Abstract(evm_request)) = &payload_v1.request
        else {
            return Err(ProverError::ChainMismatch.to_string());
        };

        let (verifier, _request_args) = build_verifier(evm_request, &payload_v1.values)?;

        verifier
            .verify_signature(&mpc_response, &public_key)
            .near_expect(ProverError::InvalidSignature);

        let log_entry_data = Self::extract_evm_log(payload_v1)?;

        Self::parse_proof_result(args.proof_kind, self.chain_kind, log_entry_data)
    }

    fn extract_evm_log(payload: &ForeignTxSignPayloadV1) -> Result<Vec<u8>, String> {
        for value in &payload.values {
            if let ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(evm_log)) = value {
                return evm_log_to_rlp(evm_log);
            }
        }

        Err(ProverError::InvalidProof.to_string())
    }

    fn parse_proof_result(
        kind: ProofKind,
        chain_kind: ChainKind,
        log_entry_data: Vec<u8>,
    ) -> Result<ProverResult, String> {
        match kind {
            ProofKind::InitTransfer => Ok(ProverResult::InitTransfer(parse_evm_event(
                chain_kind,
                log_entry_data,
            )?)),
            ProofKind::FinTransfer => Ok(ProverResult::FinTransfer(parse_evm_event(
                chain_kind,
                log_entry_data,
            )?)),
            ProofKind::DeployToken => Ok(ProverResult::DeployToken(parse_evm_event(
                chain_kind,
                log_entry_data,
            )?)),
            ProofKind::LogMetadata => Ok(ProverResult::LogMetadata(parse_evm_event(
                chain_kind,
                log_entry_data,
            )?)),
        }
    }
}

fn build_verifier(
    evm_request: &EvmRpcRequest,
    values: &[ExtractedValue],
) -> Result<
    (
        near_mpc_sdk::foreign_chain::ForeignChainSignatureVerifier,
        near_mpc_sdk::contract_interface::types::VerifyForeignTransactionRequestArgs,
    ),
    String,
> {
    let mut builder = ForeignChainRequestBuilder::new()
        .with_abstract_tx_id(evm_request.tx_id.clone())
        .with_finality(evm_request.finality.clone());

    for value in values {
        match value {
            ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(log)) => {
                builder = builder.with_expected_log(log.log_index, log.clone());
            }
            ExtractedValue::EvmExtractedValue(EvmExtractedValue::BlockHash(hash)) => {
                builder = builder.with_expected_block_hash(hash.0);
            }
            _ => return Err(ProverError::InvalidProof.to_string()),
        }
    }

    Ok(builder
        .with_derivation_path(String::new())
        .with_domain_id(DomainId::from(0u64))
        .build())
}

fn evm_log_to_rlp(
    evm_log: &near_mpc_sdk::contract_interface::types::EvmLog,
) -> Result<Vec<u8>, String> {
    use alloy::primitives::{Address, Bytes, Log, B256};
    use alloy::rlp::Encodable;

    let address = Address::from_slice(&evm_log.address.0);

    let topics: Vec<B256> = evm_log
        .topics
        .iter()
        .map(|t| B256::from_slice(&t.0))
        .collect();

    let data_str = evm_log.data.strip_prefix("0x").unwrap_or(&evm_log.data);
    let data_bytes = hex::decode(data_str).map_err(|_| ProverError::InvalidProof.to_string())?;

    let log = Log::new_unchecked(address, topics, Bytes::from(data_bytes));

    let mut buf = Vec::new();
    log.encode(&mut buf);

    Ok(buf)
}
