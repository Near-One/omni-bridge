use alloy::primitives::{Address, U256};
use alloy::sol_types::SolValue;
use near_sdk::borsh::BorshDeserialize;
use near_sdk::json_types::U128;
use near_sdk::{env, near, near_bindgen, require, PanicOnDefault};
use omni_types::prover_args::{HotInitTransfer, HotVerifyProofArgs};
use omni_types::prover_result::{InitTransferMessage, ProofKind, ProverResult};
use omni_types::utils::keccak256;
use omni_types::{ChainKind, Fee, OmniAddress, H160};

type EcdsaPublicKey = [u8; 64];

type H256 = [u8; 32];

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct HotProver {
    pub chain_kind: ChainKind,
    pub chain_id: u64,
    pub omni_bridge_address: H160,
    pub mpc_public_key: EcdsaPublicKey,
}

#[near_bindgen]
impl HotProver {
    #[init]
    #[private]
    #[must_use]
    pub fn init(
        chain_kind: ChainKind,
        chain_id: u64,
        omni_bridge_address: H160,
        mpc_public_key: Vec<u8>,
    ) -> Self {
        require!(chain_kind.is_evm_chain(), "ERR_CHAIN_KIND_NOT_EVM");
        let mpc_public_key: EcdsaPublicKey = mpc_public_key
            .try_into()
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_MPC_PUBLIC_KEY"));
        Self {
            chain_kind,
            chain_id,
            omni_bridge_address,
            mpc_public_key,
        }
    }

    /// # Errors
    ///
    /// This function will return an error if the proof is invalid.
    #[allow(clippy::needless_pass_by_value)]
    #[handle_result]
    #[result_serializer(borsh)]
    pub fn verify_proof(
        &self,
        #[serializer(borsh)] input: Vec<u8>,
    ) -> Result<ProverResult, String> {
        let args = HotVerifyProofArgs::try_from_slice(&input).map_err(|_| "ERR_PARSE_ARGS")?;
        if args.proof_kind != ProofKind::InitTransfer {
            return Err("ERR_UNSUPPORTED_PROOF_KIND".to_owned());
        }

        let transfer = &args.transfer;
        let msg_hash = self.compute_transfer_hash(transfer);
        if !validate_ecdsa_signature_with_public_key(
            &args.signature,
            &msg_hash,
            &self.mpc_public_key,
        ) {
            return Err("ERR_INVALID_SIGNATURE".to_owned());
        }

        let recipient: OmniAddress = transfer
            .recipient
            .parse()
            .map_err(|_| "ERR_INVALID_RECIPIENT")?;
        let token = OmniAddress::new_from_evm_address(self.chain_kind, transfer.token_address)
            .map_err(|_| "ERR_INVALID_TOKEN_ADDRESS")?;
        let sender = OmniAddress::new_from_evm_address(self.chain_kind, transfer.sender)
            .map_err(|_| "ERR_INVALID_SENDER_ADDRESS")?;
        let emitter_address =
            OmniAddress::new_from_evm_address(self.chain_kind, self.omni_bridge_address)
                .map_err(|_| "ERR_INVALID_EMITTER_ADDRESS")?;

        Ok(ProverResult::InitTransfer(InitTransferMessage {
            origin_nonce: transfer.origin_nonce,
            token,
            amount: U128(transfer.amount),
            recipient,
            fee: Fee {
                fee: U128(transfer.fee),
                native_fee: U128(transfer.native_fee),
            },
            sender,
            msg: transfer.message.clone(),
            emitter_address,
        }))
    }

    fn compute_transfer_hash(&self, transfer: &HotInitTransfer) -> H256 {
        let encoded = (
            U256::from(self.chain_id),
            Address::from_slice(&self.omni_bridge_address.0),
            Address::from_slice(&transfer.sender.0),
            Address::from_slice(&transfer.token_address.0),
            transfer.origin_nonce,
            transfer.amount,
            transfer.fee,
            transfer.native_fee,
            transfer.recipient.as_str(),
            transfer.message.as_str(),
        )
            .abi_encode();

        keccak256(&encoded)
    }
}

fn recover_public_keys(signature: &[u8; 64], hash: &H256) -> Vec<EcdsaPublicKey> {
    let mut result = Vec::new();
    for v in [0, 1] {
        if let Some(actual_public_key) = env::ecrecover(hash, signature, v, true) {
            result.push(actual_public_key);
        }
    }
    result
}

fn validate_ecdsa_signature_with_public_key(
    signature: &[u8; 64],
    hash: &H256,
    public_key: &EcdsaPublicKey,
) -> bool {
    let public_keys = recover_public_keys(signature, hash);
    public_keys.contains(public_key)
}
