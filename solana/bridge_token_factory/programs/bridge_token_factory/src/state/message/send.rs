use std::io::{BufWriter, Write};

use anchor_lang::prelude::*;
use near_sdk::json_types::U128;
use super::{Payload, PayloadType, DEFAULT_SERIALIZER_CAPACITY};
use crate::error::ErrorCode;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SendPayload {
    pub amount: u128,
    pub recipient: String,
}

impl Payload for SendPayload {
    type AdditionalParams = Pubkey;

    fn serialize_for_near(&self, mint: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        PayloadType::Send.serialize(&mut writer)?;
        mint.to_string().serialize(&mut writer)?;
        near_sdk::borsh::BorshSerialize::serialize(&U128(self.amount), &mut writer)?;
        writer.write(&[2])?;
        self.recipient.to_string().serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}