use std::io::{BufWriter, Write};

use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use near_sdk::json_types::U128;

use super::{Payload, PayloadType, DEFAULT_SERIALIZER_CAPACITY};

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
        PayloadType::Deposit.serialize(&mut writer)?;
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
    pub nonce: u128,
}

impl Payload for FinalizeDepositResponse {
    type AdditionalParams = ();
    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        PayloadType::DepositResponse.serialize(&mut writer)?;
        self.serialize(&mut writer)?; // borsh encoding
        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
