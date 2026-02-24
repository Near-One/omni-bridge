use borsh::BorshDeserialize;
use near_sdk::{env, near, near_bindgen, require, PanicOnDefault};

use contract_interface::types::{
    EvmExtractedValue, ExtractedValue, ForeignTxSignPayload, ForeignTxSignPayloadV1,
};

use omni_types::evm::events::parse_evm_event;
use omni_types::prover_args::MpcVerifyProofArgs;
use omni_types::prover_result::{ProofKind, ProverResult};
use omni_types::ChainKind;

mod verify;

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MpcOmniProver {
    pub mpc_public_key: String,
    pub chain_kind: ChainKind,
}

#[near_bindgen]
impl MpcOmniProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(mpc_public_key: String, chain_kind: ChainKind) -> Self {
        require!(
            chain_kind.is_evm_chain(),
            "MPC prover only supports EVM chains"
        );

        let pk_bytes = hex::decode(&mpc_public_key).expect("Invalid hex for MPC public key");
        require!(
            pk_bytes.len() == 33,
            "MPC public key must be 33 bytes (compressed secp256k1)"
        );

        Self {
            mpc_public_key,
            chain_kind,
        }
    }

    #[private]
    pub fn update_mpc_public_key(&mut self, mpc_public_key: String) {
        let pk_bytes = hex::decode(&mpc_public_key).expect("Invalid hex for MPC public key");
        require!(
            pk_bytes.len() == 33,
            "MPC public key must be 33 bytes (compressed secp256k1)"
        );

        env::log_str(&format!(
            "MPC public key updated from {} to {}",
            self.mpc_public_key, mpc_public_key
        ));
        self.mpc_public_key = mpc_public_key;
    }

    #[allow(clippy::needless_pass_by_value)]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof(
        &self,
        #[serializer(borsh)] input: Vec<u8>,
    ) -> Result<ProverResult, String> {
        let args = MpcVerifyProofArgs::try_from_slice(&input)
            .map_err(|e| format!("Failed to parse MpcVerifyProofArgs: {e}"))?;

        let sign_payload = ForeignTxSignPayload::try_from_slice(&args.sign_payload)
            .map_err(|e| format!("Failed to parse ForeignTxSignPayload: {e}"))?;

        let computed_hash = sign_payload
            .compute_msg_hash()
            .map_err(|e| format!("Failed to compute payload hash: {e}"))?;

        require!(
            computed_hash.0 == args.payload_hash,
            "Payload hash mismatch: computed vs provided"
        );

        let mpc_pk_bytes = hex::decode(&self.mpc_public_key)
            .map_err(|e| format!("Invalid MPC public key hex: {e}"))?;

        verify::verify_secp256k1_signature(
            &mpc_pk_bytes,
            &args.payload_hash,
            &args.signature_big_r,
            &args.signature_s,
            args.signature_recovery_id,
        )?;

        let ForeignTxSignPayload::V1(payload_v1) = sign_payload;
        let log_entry_data = Self::extract_evm_log(&payload_v1)?;

        Self::parse_proof_result(args.proof_kind, self.chain_kind, log_entry_data)
    }

    fn extract_evm_log(payload: &ForeignTxSignPayloadV1) -> Result<Vec<u8>, String> {
        for value in &payload.values {
            if let ExtractedValue::EvmExtractedValue(EvmExtractedValue::Log(evm_log)) = value {
                return evm_log_to_rlp(evm_log);
            }
        }
        Err("No EVM log found in MPC extracted values".to_string())
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

fn evm_log_to_rlp(evm_log: &contract_interface::types::EvmLog) -> Result<Vec<u8>, String> {
    use alloy::primitives::{Address, Bytes, Log, B256};
    use alloy::rlp::Encodable;

    let address = Address::from_slice(&evm_log.address.0);

    let topics: Vec<B256> = evm_log
        .topics
        .iter()
        .map(|t| B256::from_slice(&t.0))
        .collect();

    let data_str = evm_log.data.strip_prefix("0x").unwrap_or(&evm_log.data);
    let data_bytes =
        hex::decode(data_str).map_err(|e| format!("Invalid hex in EVM log data: {e}"))?;

    let log = Log::new_unchecked(address, topics, Bytes::from(data_bytes));

    let mut buf = Vec::new();
    log.encode(&mut buf);

    Ok(buf)
}

#[cfg(test)]
mod tests;
