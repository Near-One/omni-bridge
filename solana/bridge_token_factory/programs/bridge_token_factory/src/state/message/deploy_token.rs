use std::io::BufWriter;

use anchor_lang::prelude::*;

use super::{Payload, PayloadType, DEFAULT_SERIALIZER_CAPACITY};
use crate::error::ErrorCode;

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
        PayloadType::DeployToken.serialize(&mut writer)?;
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
}

impl Payload for DeployTokenResponse {
    type AdditionalParams = ();

    fn serialize_for_near(&self, _params: Self::AdditionalParams) -> Result<Vec<u8>> {
        let mut writer = BufWriter::new(Vec::with_capacity(DEFAULT_SERIALIZER_CAPACITY));
        PayloadType::DeployTokenResponse.serialize(&mut writer)?;
        self.serialize(&mut writer)?; // borsh encoding
        writer
            .into_inner()
            .map_err(|_| error!(ErrorCode::InvalidArgs))
    }
}