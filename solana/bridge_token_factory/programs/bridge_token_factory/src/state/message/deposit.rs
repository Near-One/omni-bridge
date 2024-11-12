use std::io::{BufWriter, Write};

use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use near_sdk::json_types::U128;

use super::{IncomingMessageType, OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DepositPayload {
    pub nonce: u128,
    pub token: String,
    pub amount: u128,
    pub fee_recipient: Option<String>,
}

impl Payload for DepositPayload {
    type AdditionalParams = Pubkey;
    fn serialize_for_near(&self, recipient: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        IncomingMessageType::InitTransfer.serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        self.token.serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.amount), &mut writer)?;
        writer.write(&[2])?;
        recipient.to_string().serialize(&mut writer)?;
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
        format!("sol:{:?}", self.token).serialize(&mut writer)?;
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
