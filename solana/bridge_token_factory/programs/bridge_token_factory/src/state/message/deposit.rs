use std::io::{BufWriter, Write};

use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};
use anchor_lang::prelude::*;
use near_sdk::json_types::U128;

use super::{IncomingMessageType, OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DepositPayload {
    pub nonce: u128,
    pub amount: u128,
    pub fee_recipient: Option<String>,
}

impl Payload for DepositPayload {
    type AdditionalParams = (Pubkey, Pubkey); // mint, recipient
    fn serialize_for_near(&self, params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. prefix
        IncomingMessageType::InitTransfer.serialize(&mut writer)?;
        // 1. nonce
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        // 2. token
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.0.serialize(&mut writer)?;
        // 3. amount
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.amount), &mut writer)?;
        // 4. recipient
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.1.to_string().serialize(&mut writer)?;
        // 5. fee_recipient
        self.fee_recipient.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct FinalizeDepositResponse {
    pub token: Pubkey,
    pub amount: u128,
    pub fee_recipient: String,
    pub nonce: u128,
}

impl Payload for FinalizeDepositResponse {
    type AdditionalParams = ();
    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. OutgoingMessageType::FinTransfer
        OutgoingMessageType::FinTransfer.serialize(&mut writer)?;
        // 1. token
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        self.token.serialize(&mut writer)?;
        // 2. amount
        Self::serialize_as_near_u128(self.amount, &mut writer)?;
        // 3. recipient
        self.fee_recipient.serialize(&mut writer)?;
        // 4. nonce
        Self::serialize_as_near_u128(self.nonce, &mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
