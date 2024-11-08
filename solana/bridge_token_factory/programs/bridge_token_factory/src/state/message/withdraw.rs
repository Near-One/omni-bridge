use std::io::{BufWriter, Write};

use anchor_lang::prelude::*;
use near_sdk::json_types::U128;

use super::{Payload, PayloadType, DEFAULT_SERIALIZER_CAPACITY};
use crate::error::ErrorCode;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawPayload {
    pub nonce: u128,
    pub amount: u128,
    pub fee_recipient: Option<String>,
}

impl Payload for WithdrawPayload {
    type AdditionalParams = (Pubkey, Pubkey);

    fn serialize_for_near(
        &self,
        (recipient, mint): Self::AdditionalParams,
    ) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        PayloadType::Withdraw.serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        mint.to_string().serialize(&mut writer)?;
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
pub struct FinalizeWithdrawResponse {
    pub nonce: u128,
}

impl Payload for FinalizeWithdrawResponse {
    type AdditionalParams = ();

    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        PayloadType::WithdrawResponse.serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.nonce), &mut writer)?;
        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}