use anchor_lang::prelude::*;
use anchor_lang::solana_program::{keccak, secp256k1_recover::secp256k1_recover};

use crate::error::ErrorCode;

pub mod deploy_token;
pub mod finalize_transfer;
pub mod init_transfer;
pub mod log_metadata;

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
    pub fn verify_signature(
        &self,
        params: P::AdditionalParams,
        derived_near_bridge_address: &[u8; 64],
    ) -> Result<()> {
        let serialized = self.payload.serialize_for_near(params)?;
        let hash = keccak::hash(&serialized);

        let signature_bytes = &self.signature[0..64];

        let signature = libsecp256k1::Signature::parse_standard_slice(signature_bytes)
            .map_err(|_| ProgramError::InvalidArgument)?;
        require!(!signature.s.is_high(), ErrorCode::MalleableSignature);

        let signer = secp256k1_recover(&hash.to_bytes(), self.signature[64], signature_bytes)
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
    LogMetadata,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug)]
pub struct TransferId {
    pub origin_chain: u8,
    pub origin_nonce: u64,
}

const DEFAULT_SERIALIZER_CAPACITY: usize = 1024;
