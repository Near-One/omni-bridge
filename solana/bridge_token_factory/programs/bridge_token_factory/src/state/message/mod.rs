use std::io::BufWriter;

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};
use near_sdk::json_types::{U128, U64};

use crate::error::ErrorCode;

pub mod deploy_token;
pub mod finalize_transfer;
pub mod init_transfer;

pub trait Payload: AnchorSerialize + AnchorDeserialize {
    type AdditionalParams;

    fn serialize_for_near(&self, params: Self::AdditionalParams) -> Result<Vec<u8>>;

    fn serialize_as_near_u128(u: u128, writer: &mut BufWriter<Vec<u8>>) -> Result<()> {
        near_sdk::borsh::BorshSerialize::serialize(&U128(u), writer)?;
        Ok(())
    }

    fn serialize_as_near_u64(u: u64, writer: &mut BufWriter<Vec<u8>>) -> Result<()> {
        near_sdk::borsh::BorshSerialize::serialize(&U64(u), writer)?;
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SignedPayload<P: Payload> {
    pub payload: P,
    pub signature: [u8; 65],
}

impl<P: Payload> SignedPayload<P> {
    pub fn verify_signature(
        &self,
        params: P::AdditionalParams,
        derived_near_bridge_address: &[u8; 64],
    ) -> Result<()> {
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
pub enum IncomingMessageType {
    InitTransfer,
    Metadata,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub enum OutgoingMessageType {
    InitTransfer,
    FinTransfer,
    DeployToken,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug)]
pub struct TransferId {
    pub origin_chain: u8,
    pub origin_nonce: u64,
}

const DEFAULT_SERIALIZER_CAPACITY: usize = 1024;
