use std::collections::HashMap;

use alloy::{
    primitives::{Address, Bytes, Log, B256},
    rlp::Encodable,
};
use borsh::BorshDeserialize;
use near_mpc_sdk::{
    contract_interface::types::{
        EvmExtractedValue, EvmFinality, ExtractedValue, ForeignChainRpcRequest,
        ForeignTxSignPayload, ForeignTxSignPayloadV1, StarknetExtractedValue, StarknetFinality,
        VerifyForeignTransactionRequestArgs, VerifyForeignTransactionResponse,
    },
    sign::DomainId,
};
use near_sdk::{ext_contract, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise};
use omni_types::{
    errors::ProverError,
    evm::events::parse_evm_proof,
    prover_args::MpcVerifyProofArgs,
    prover_result::{ProofKind, ProverResult},
    starknet::events::parse_starknet_proof,
    ChainKind,
};
use omni_utils::near_expect::NearExpect;

#[cfg(test)]
mod tests;

const FOREIGN_TX_DOMAIN_ID: u64 = 3;
const PAYLOAD_VERSION: u8 = 1;

const VERIFY_FOREIGN_TX_GAS: Gas = Gas::from_tgas(20);
const VERIFY_CALLBACK_GAS: Gas = Gas::from_tgas(7);
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[ext_contract(ext_mpc_contract)]
pub trait MpcContract {
    fn verify_foreign_transaction(&mut self, request: VerifyForeignTransactionRequestArgs);
}

/// Finality enum that supports both EVM and Starknet chains.
#[near(serializers = [borsh, json])]
#[derive(Debug, Clone, PartialEq)]
pub enum MpcFinality {
    Evm(EvmFinality),
    Starknet(StarknetFinality),
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MpcOmniProver {
    pub mpc_contract_id: AccountId,
    pub finalities: HashMap<ChainKind, MpcFinality>,
}

#[near]
impl MpcOmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(mpc_contract_id: AccountId) -> Self {
        let mut finalities = HashMap::new();
        finalities.insert(ChainKind::Abs, MpcFinality::Evm(EvmFinality::Safe));
        finalities.insert(
            ChainKind::Strk,
            MpcFinality::Starknet(StarknetFinality::AcceptedOnL2),
        );

        Self {
            mpc_contract_id,
            finalities,
        }
    }

    pub fn get_finality(&self, chain_kind: ChainKind) -> Option<MpcFinality> {
        self.finalities.get(&chain_kind).cloned()
    }

    pub fn get_finalities(&self) -> Vec<(&ChainKind, &MpcFinality)> {
        self.finalities.iter().collect()
    }

    #[private]
    pub fn set_finality(&mut self, chain_kind: ChainKind, finality: MpcFinality) {
        self.finalities.insert(chain_kind, finality);
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Promise {
        let args = MpcVerifyProofArgs::try_from_slice(&input).near_expect(ProverError::ParseArgs);

        let sign_payload = ForeignTxSignPayload::try_from_slice(&args.sign_payload)
            .near_expect(ProverError::ParseArgs);

        let ForeignTxSignPayload::V1(ref payload_v1) = sign_payload;

        let chain_kind = Self::request_to_chain_kind(&payload_v1.request)
            .near_expect(ProverError::UnsupportedChain);

        let finality = self
            .finalities
            .get(&chain_kind)
            .near_expect(ProverError::UnsupportedChain);

        require!(
            Self::request_matches_finality(&payload_v1.request, finality),
            ProverError::FinalityMismatch.as_ref()
        );

        let request_args = VerifyForeignTransactionRequestArgs {
            request: payload_v1.request.clone(),
            derivation_path: String::new(),
            domain_id: DomainId(FOREIGN_TX_DOMAIN_ID),
            payload_version: PAYLOAD_VERSION,
        };

        ext_mpc_contract::ext(self.mpc_contract_id.clone())
            .with_static_gas(VERIFY_FOREIGN_TX_GAS)
            .with_attached_deposit(ONE_YOCTO)
            .verify_foreign_transaction(request_args)
            .then(
                Self::ext(near_sdk::env::current_account_id())
                    .with_static_gas(VERIFY_CALLBACK_GAS)
                    .verify_callback(args.proof_kind, args.sign_payload, chain_kind),
            )
    }

    #[allow(clippy::needless_pass_by_value)]
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
            .map_err(|_| ProverError::ParseArgs.to_string())?;

        let expected_hash = sign_payload
            .compute_msg_hash()
            .map_err(|_| ProverError::InvalidPayloadHash.to_string())?;

        if expected_hash != mpc_response.payload_hash {
            return Err(ProverError::InvalidPayloadHash.to_string());
        }

        let ForeignTxSignPayload::V1(ref payload_v1) = sign_payload;

        if chain_kind == ChainKind::Strk {
            Self::parse_starknet_result(proof_kind, chain_kind, payload_v1)
        } else {
            let log_entry_data = Self::extract_evm_log(payload_v1)?;
            parse_evm_proof(proof_kind, chain_kind, log_entry_data)
        }
    }

    fn request_to_chain_kind(request: &ForeignChainRpcRequest) -> Option<ChainKind> {
        match request {
            ForeignChainRpcRequest::Abstract(_) => Some(ChainKind::Abs),
            ForeignChainRpcRequest::Ethereum(_) => Some(ChainKind::Eth),
            ForeignChainRpcRequest::Starknet(_) => Some(ChainKind::Strk),
            _ => None,
        }
    }

    fn request_matches_finality(request: &ForeignChainRpcRequest, finality: &MpcFinality) -> bool {
        match (request, finality) {
            (
                ForeignChainRpcRequest::Ethereum(args) | ForeignChainRpcRequest::Abstract(args),
                MpcFinality::Evm(finality),
            ) => args.finality == *finality,
            (ForeignChainRpcRequest::Starknet(args), MpcFinality::Starknet(finality)) => {
                args.finality == *finality
            }
            _ => false,
        }
    }

    fn extract_evm_log(payload: &ForeignTxSignPayloadV1) -> Result<Vec<u8>, String> {
        if payload.values.len() != 1 {
            return Err(ProverError::InvalidPayloadValuesLength.to_string());
        }

        let Some(ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(evm_log))) =
            payload.values.first()
        else {
            return Err(ProverError::InvalidProof.to_string());
        };

        evm_log_to_rlp(evm_log)
    }

    fn parse_starknet_result(
        kind: ProofKind,
        chain_kind: ChainKind,
        payload: &ForeignTxSignPayloadV1,
    ) -> Result<ProverResult, String> {
        if payload.values.len() != 1 {
            return Err(ProverError::InvalidPayloadValuesLength.to_string());
        }

        let Some(ExtractedValue::StarknetExtractedValue(StarknetExtractedValue::Log(starknet_log))) =
            payload.values.first()
        else {
            return Err(ProverError::InvalidProof.to_string());
        };

        let keys: Vec<[u8; 32]> = starknet_log.keys.iter().map(|k| k.0).collect();
        let data: Vec<[u8; 32]> = starknet_log.data.iter().map(|d| d.0).collect();

        parse_starknet_proof(kind, chain_kind, &starknet_log.from_address.0, &keys, &data)
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
