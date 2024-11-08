use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};

use crate::error::ErrorCode;

pub mod deploy_token;
pub mod deposit;
pub mod withdraw;
pub mod repay;
pub mod send;

pub trait Payload: AnchorSerialize + AnchorDeserialize {
    type AdditionalParams;

    fn serialize_for_near(&self, params: Self::AdditionalParams) -> Result<Vec<u8>>;
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SignedPayload<P: Payload> {
    pub payload: P,
    pub signature: [u8; 65],
}

impl<P: Payload> SignedPayload<P> {
    pub fn verify_signature(&self, params: P::AdditionalParams, derived_near_bridge_address: &[u8; 64]) -> Result<()> {
        let serialized = self.payload.serialize_for_near(params)?;
        let hash = keccak::hash(&serialized);

        let signer =
            secp256k1_recover(&hash.to_bytes(), self.signature[64], &self.signature[0..64])
                .map_err(|_| error!(ErrorCode::SignatureVerificationFailed))?;

        require!(
            signer.0 == *derived_near_bridge_address,
            ErrorCode::SignatureVerificationFailed
        );

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub enum PayloadType {
    DeployToken,
    DeployTokenResponse,
    Deposit,
    DepositResponse,
    Withdraw,
    WithdrawResponse,
    Repay,
    RepayResponse,
    Send,
    SendResponse,
}

const DEFAULT_SERIALIZER_CAPACITY: usize = 1024;