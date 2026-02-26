use alloy::{
    primitives::{Address, Bytes, Log, B256},
    rlp::Encodable,
};
use borsh::BorshDeserialize;
use near_mpc_sdk::contract_interface::types::{
    EvmExtractedValue, ExtractedValue, ForeignChainRpcRequest, ForeignTxSignPayload,
    ForeignTxSignPayloadV1, VerifyForeignTransactionRequestArgs, VerifyForeignTransactionResponse,
};
use near_sdk::{
    ext_contract, near, require, serde_json, AccountId, Gas, NearToken, PanicOnDefault, Promise,
};
use omni_types::{
    errors::ProverError,
    evm::events::parse_evm_event,
    prover_args::MpcVerifyProofArgs,
    prover_result::{ProofKind, ProverResult},
    ChainKind,
};
use omni_utils::near_expect::NearExpect;

#[cfg(test)]
mod tests;

const VERIFY_FOREIGN_TX_GAS: Gas = Gas::from_tgas(20);
const VERIFY_CALLBACK_GAS: Gas = Gas::from_tgas(7);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[ext_contract(ext_mpc_contract)]
pub trait MpcContract {
    fn verify_foreign_transaction(&mut self, request: VerifyForeignTransactionRequestArgs);
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MpcOmniProver {
    pub mpc_contract_id: AccountId,
    pub chain_kind: ChainKind,
}

#[near]
impl MpcOmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(mpc_contract_id: AccountId, chain_kind: ChainKind) -> Self {
        require!(
            chain_kind.is_evm_chain(),
            ProverError::UnsupportedChain.as_ref()
        );

        Self {
            mpc_contract_id,
            chain_kind,
        }
    }

    #[private]
    pub fn update_mpc_contract_id(&mut self, mpc_contract_id: AccountId) {
        self.mpc_contract_id = mpc_contract_id;
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Promise {
        let args = MpcVerifyProofArgs::try_from_slice(&input).near_expect(ProverError::ParseArgs);

        let sign_payload = ForeignTxSignPayload::try_from_slice(&args.sign_payload)
            .near_expect(ProverError::ParseArgs);

        let ForeignTxSignPayload::V1(ref payload_v1) = sign_payload;

        require!(
            Self::request_matches_chain(&payload_v1.request, self.chain_kind),
            ProverError::ChainMismatch.as_ref()
        );

        let request_args: VerifyForeignTransactionRequestArgs =
            serde_json::from_str(&args.request_args_json).near_expect(ProverError::ParseArgs);

        ext_mpc_contract::ext(self.mpc_contract_id.clone())
            .with_static_gas(VERIFY_FOREIGN_TX_GAS)
            .with_attached_deposit(ONE_YOCTO)
            .verify_foreign_transaction(request_args)
            .then(
                Self::ext(near_sdk::env::current_account_id())
                    .with_static_gas(VERIFY_CALLBACK_GAS)
                    .verify_callback(args.proof_kind, args.sign_payload, self.chain_kind),
            )
    }

    #[private]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_callback(
        &self,
        #[serializer(borsh)] proof_kind: ProofKind,
        #[serializer(borsh)] sign_payload_bytes: Vec<u8>,
        #[serializer(borsh)] chain_kind: ChainKind,
        #[callback_result] call_result: Result<
            VerifyForeignTransactionResponse,
            near_sdk::PromiseError,
        >,
    ) -> Result<ProverResult, String> {
        let mpc_response = call_result.map_err(|_| ProverError::InvalidProof.to_string())?;

        let sign_payload = ForeignTxSignPayload::try_from_slice(&sign_payload_bytes)
            .near_expect(ProverError::ParseArgs);

        let expected_hash = sign_payload
            .compute_msg_hash()
            .map_err(|_| ProverError::InvalidPayloadHash.to_string())?;

        if expected_hash != mpc_response.payload_hash {
            return Err(ProverError::InvalidPayloadHash.to_string());
        }

        let ForeignTxSignPayload::V1(ref payload_v1) = sign_payload;

        let log_entry_data = Self::extract_evm_log(payload_v1)?;

        Self::parse_proof_result(proof_kind, chain_kind, log_entry_data)
    }

    fn request_matches_chain(request: &ForeignChainRpcRequest, chain_kind: ChainKind) -> bool {
        matches!(
            (request, chain_kind),
            (ForeignChainRpcRequest::Abstract(_), ChainKind::Abs)
                | (ForeignChainRpcRequest::Ethereum(_), ChainKind::Eth)
        )
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

fn evm_log_to_rlp(
    evm_log: &near_mpc_sdk::contract_interface::types::EvmLog,
) -> Result<Vec<u8>, String> {
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
