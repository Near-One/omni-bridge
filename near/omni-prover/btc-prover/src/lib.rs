use borsh::BorshDeserialize;
use hex;
use near_sdk::{
    env, ext_contract, near, near_bindgen, AccountId, Gas, PanicOnDefault, Promise,
    promise_result_as_success, require, serde_json
};
use omni_types::prover_args::{BtcVerifyProofArgs, BtcProof};
use omni_types::prover_result::{ProverResult, BtcFinTransferMessage};
use omni_types::{ChainKind, TransferId};

const VERIFY_PROOF_CALLBACK_GAS: Gas = Gas::from_tgas(5);
const BLOCK_HASH_SAFE_GAS: Gas = Gas::from_tgas(5);

/// Defines an interface to call EthClient contract to get the safe block hash for a given block
/// number. It returns Some(hash) if the block hash is present in the safe canonical chain, or
/// None if the block number is not part of the canonical chain yet.
#[ext_contract(evm_client)]
pub trait BtcClient {
    #[result_serializer(borsh)]
    fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: BtcProof) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct BtcProver {
    pub light_client: AccountId,
    pub chain_kind: ChainKind,
}

#[near_bindgen]
impl BtcProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(light_client: AccountId, chain_kind: ChainKind) -> Self {
        Self {
            light_client,
            chain_kind,
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    #[handle_result]
    pub fn verify_proof(&self, #[serializer(borsh)] input: Vec<u8>) -> Result<Promise, String> {
        let args = BtcVerifyProofArgs::try_from_slice(&input).map_err(|_| "ERR_PARSE_ARGS")?;
        let btc_proof = args.proof;

        // Verify block header was in the bridge
        Ok(evm_client::ext(self.light_client.clone())
            .with_static_gas(BLOCK_HASH_SAFE_GAS)
            .verify_transaction_inclusion(btc_proof.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_PROOF_CALLBACK_GAS)
                    .verify_proof_callback(
                        btc_proof,
                        args.transfer_id
                    ),
            ))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[private]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof_callback(
        &mut self,
        #[serializer(borsh)] btc_proof: BtcProof,
        #[serializer(borsh)] transfer_id: TransferId,
    ) -> Result<ProverResult, String> {
        let result_bytes =
            promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");

        Ok(ProverResult::BtcFinTransfer(BtcFinTransferMessage {
            btc_tx_hash: hex::encode(btc_proof.tx_id.0.into_iter().rev().collect::<Vec<_>>()),
            transfer_id,
        }))
    }
}
