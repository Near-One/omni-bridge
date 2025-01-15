use std::io::{BufWriter, Write};

use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};
use anchor_lang::prelude::*;

use super::{
    IncomingMessageType, OutgoingMessageType, Payload, TransferId, DEFAULT_SERIALIZER_CAPACITY,
};

#[derive(AnchorSerialize, AnchorDeserialize, Debug)]
pub struct FinalizeTransferPayload {
    pub destination_nonce: u64,
    pub transfer_id: TransferId,
    pub amount: u128,
    pub fee_recipient: Option<String>,
}

impl Payload for FinalizeTransferPayload {
    type AdditionalParams = (Pubkey, Pubkey); // mint, recipient
    fn serialize_for_near(&self, params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. prefix
        IncomingMessageType::InitTransfer.serialize(&mut writer)?;
        // 1. destination_nonce
        self.destination_nonce.serialize(&mut writer)?;
        // 2. transfer_id
        writer.write(&[self.transfer_id.origin_chain])?;
        self.transfer_id.origin_nonce.serialize(&mut writer)?;
        // 3. token
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.0.serialize(&mut writer)?;
        // 4. amount
        self.amount.serialize(&mut writer)?;
        // 5. recipient
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.1.serialize(&mut writer)?;
        // 6. fee_recipient
        self.fee_recipient.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeTransferResponse {
    pub token: Pubkey,
    pub amount: u128,
    pub fee_recipient: String,
    pub transfer_id: TransferId,
}

impl Payload for FinalizeTransferResponse {
    type AdditionalParams = ();
    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. OutgoingMessageType::FinTransfer
        OutgoingMessageType::FinTransfer.serialize(&mut writer)?;
        // 1. transfer_id
        writer.write(&[self.transfer_id.origin_chain])?;
        self.transfer_id.origin_nonce.serialize(&mut writer)?;
        // 2. token
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        self.token.serialize(&mut writer)?;
        // 3. amount
        self.amount.serialize(&mut writer)?;
        // 4. fee_recipient
        self.fee_recipient.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
