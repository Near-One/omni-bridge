use std::io::{BufWriter, Write};

use super::{OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};
use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SendPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
}

impl Payload for SendPayload {
    type AdditionalParams = (u64, Pubkey, Pubkey); // nonce, sender, token_address

    fn serialize_for_near(&self, params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. OutgoingMessageType::InitTransfer
        OutgoingMessageType::InitTransfer.serialize(&mut writer)?;
        // 1. sender
        writer.write_all(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.1.serialize(&mut writer)?;
        // 2. token
        writer.write_all(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        params.2.serialize(&mut writer)?;
        // 3. nonce
        Self::serialize_as_near_u128(params.0.into(), &mut writer)?;
        // 4. amount
        Self::serialize_as_near_u128(self.amount, &mut writer)?;
        // 5. fee
        Self::serialize_as_near_u128(self.fee, &mut writer)?;
        // 6. native_fee
        Self::serialize_as_near_u128(0, &mut writer)?;
        // 7. recipient
        self.recipient.serialize(&mut writer)?;
        // 8. message
        writer.write(&[0])?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
