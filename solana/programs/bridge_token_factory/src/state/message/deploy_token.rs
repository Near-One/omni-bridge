use std::io::{BufWriter, Write};

use anchor_lang::prelude::*;

use super::{IncomingMessageType, OutgoingMessageType, Payload, DEFAULT_SERIALIZER_CAPACITY};
use crate::{constants::SOLANA_OMNI_BRIDGE_CHAIN_ID, error::ErrorCode};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DeployTokenPayload {
    pub token: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

impl Payload for DeployTokenPayload {
    type AdditionalParams = ();

    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        IncomingMessageType::Metadata.serialize(&mut writer)?;
        self.serialize(&mut writer)?; // borsh encoding
        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DeployTokenResponse {
    pub token: String,
    pub solana_mint: Pubkey,
    pub decimals: u8,
    pub origin_decimals: u8,
}

impl Payload for DeployTokenResponse {
    type AdditionalParams = ();

    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        // 0. OutgoingMessageType::DeployToken
        OutgoingMessageType::DeployToken.serialize(&mut writer)?;
        // 1. token
        self.token.serialize(&mut writer)?;
        // 2. solana_mint
        writer.write_all(&[SOLANA_OMNI_BRIDGE_CHAIN_ID])?;
        self.solana_mint.serialize(&mut writer)?;
        // 3. decimals
        self.decimals.serialize(&mut writer)?;
        // 4. origin_decimals
        self.origin_decimals.serialize(&mut writer)?;

        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}
