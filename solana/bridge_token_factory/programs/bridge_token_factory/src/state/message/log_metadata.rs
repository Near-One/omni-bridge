use std::io::{BufWriter, Write};

use super::{OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};
use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct LogMetadataPayload {
    pub token: Pubkey,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

impl Payload for LogMetadataPayload {
    type AdditionalParams = ();

    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. Message type
        OutgoingMessageType::LogMetadata.serialize(&mut writer)?;
        // 1. token
        writer.write(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        self.token.serialize(&mut writer)?;
        // 2. name
        self.name.serialize(&mut writer)?;
        // 3. symbol
        self.symbol.serialize(&mut writer)?;
        // 4. decimals
        writer.write(&[self.decimals])?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}