use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Gas, PanicOnDefault, Promise, PromiseError,
};
use omni_types::prover_types::ProofResult;
use omni_types::ChainKind;

mod byte_utils;
mod parsed_vaa;

/// Gas to call verify_log_entry on prover.
pub const VERIFY_LOG_ENTRY_GAS: Gas = Gas::from_tgas(50);

#[ext_contract(ext_prover)]
pub trait Prover {
    fn verify_vaa(&self, vaa: String) -> u32;
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct WormholeOmniProverProxy {
    pub prover_account: AccountId,
}

#[near_bindgen]
impl WormholeOmniProverProxy {
    #[init]
    #[private]
    #[must_use]
    pub fn init(prover_account: AccountId) -> Self {
        Self { prover_account }
    }

    pub fn verify_proof(&self, chain_kind: ChainKind, msg: Vec<u8>) -> Promise {
        let vaa = String::from_utf8(msg).unwrap_or_else(|_| env::panic_str("ErrorOnVaaParsing"));
        env::log_str(&format!("{}", vaa));

        ext_prover::ext(self.prover_account.clone())
            .with_static_gas(VERIFY_LOG_ENTRY_GAS)
            .verify_vaa(vaa.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(VERIFY_LOG_ENTRY_GAS)
                    .verify_vaa_callback(chain_kind, vaa),
            )
    }

    #[private]
    #[handle_result]
    pub fn verify_vaa_callback(
        &mut self,
        _chain_kind: ChainKind,
        vaa: String,
        #[callback_result] gov_idx: Result<u32, PromiseError>,
    ) -> Result<ProofResult, String> {
        if gov_idx.is_err() {
            return Err("Proof is not valid!".to_owned());
        }

        let h = hex::decode(vaa).expect("invalidVaa");
        let parsed_vaa = parsed_vaa::ParsedVAA::parse(&h);
        let _data: &[u8] = &parsed_vaa.payload;
        Err("TODO: parse data".to_owned())
    }
}
