use std::io::{BufWriter, Write};

use super::{OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};
use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitTransferPayload {
    pub amount: u128,
    pub recipient: String,
    pub fee: u128,
    pub native_fee: u64,
    pub message: String,
}

impl Payload for InitTransferPayload {
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
        params.0.serialize(&mut writer)?;
        // 4. amount
        self.amount.serialize(&mut writer)?;
        // 5. fee
        self.fee.serialize(&mut writer)?;
        // 6. native_fee
        (self.native_fee as u128).serialize(&mut writer)?;
        // 7. recipient
        self.recipient.serialize(&mut writer)?;
        // 8. message
        self.message.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
