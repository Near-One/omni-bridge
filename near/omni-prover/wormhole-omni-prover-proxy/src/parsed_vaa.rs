//https://github.com/wormhole-foundation/wormhole/blob/main/near/contracts/wormhole/src/state.rs

use {
    crate::byte_utils::ByteUtils,
    borsh::BorshDeserialize,
    near_sdk::env,
    omni_types::{
        prover_result::{
            DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage,
            ProofKind,
        },
        stringify, ChainKind, Fee, Nonce, OmniAddress, TransferId,
    },
};

// Validator Action Approval(VAA) data
#[allow(dead_code)]
pub struct ParsedVAA {
    pub version: u8,
    pub guardian_set_index: u32,
    pub timestamp: u32,
    pub nonce: u32,
    pub len_signers: usize,

    pub emitter_chain: u16,
    pub emitter_address: Vec<u8>,
    pub sequence: u64,
    pub consistency_level: u8,
    pub payload: Vec<u8>,

    pub hash: Vec<u8>,
}

impl ParsedVAA {
    /* VAA format:

    header (length 6):
    0   uint8   version (0x01)
    1   uint32  guardian set index
    5   uint8   len signatures

    per signature (length 66):
    0   uint8       index of the signer (in guardian keys)
    1   [65]uint8   signature

    body:
    0   uint32      timestamp (unix in seconds)
    4   uint32      nonce
    8   uint16      emitter_chain
    10  [32]uint8   emitter_address
    42  uint64      sequence
    50  uint8       consistency_level
    51  []uint8     payload
    */

    pub const HEADER_LEN: usize = 6;
    pub const SIGNATURE_LEN: usize = 66;

    pub const GUARDIAN_SET_INDEX_POS: usize = 1;
    pub const LEN_SIGNER_POS: usize = 5;

    pub const VAA_NONCE_POS: usize = 4;
    pub const VAA_EMITTER_CHAIN_POS: usize = 8;
    pub const VAA_EMITTER_ADDRESS_POS: usize = 10;
    pub const VAA_SEQUENCE_POS: usize = 42;
    pub const VAA_CONSISTENCY_LEVEL_POS: usize = 50;
    pub const VAA_PAYLOAD_POS: usize = 51;

    pub fn parse(data: &[u8]) -> Self {
        let version = data.get_u8(0);

        // Load 4 bytes starting from index 1
        let guardian_set_index: u32 = data.get_u32(Self::GUARDIAN_SET_INDEX_POS);
        let len_signers = data.get_u8(Self::LEN_SIGNER_POS).into();
        let body_offset: usize = Self::HEADER_LEN + Self::SIGNATURE_LEN * len_signers;

        // Hash the body
        if body_offset >= data.len() {
            env::panic_str("InvalidVAA");
        }
        let body = &data[body_offset..];

        let hash = env::keccak256(body);

        // Signatures valid, apply VAA
        if body_offset + Self::VAA_PAYLOAD_POS > data.len() {
            env::panic_str("InvalidVAA");
        }

        let timestamp = data.get_u32(body_offset);
        let nonce = data.get_u32(body_offset + Self::VAA_NONCE_POS);
        let emitter_chain = data.get_u16(body_offset + Self::VAA_EMITTER_CHAIN_POS);
        let emitter_address = data
            .get_bytes32(body_offset + Self::VAA_EMITTER_ADDRESS_POS)
            .to_vec();
        let sequence = data.get_u64(body_offset + Self::VAA_SEQUENCE_POS);
        let consistency_level = data.get_u8(body_offset + Self::VAA_CONSISTENCY_LEVEL_POS);
        let payload = data[body_offset + Self::VAA_PAYLOAD_POS..].to_vec();

        Self {
            version,
            guardian_set_index,
            timestamp,
            nonce,
            len_signers,
            emitter_chain,
            emitter_address,
            sequence,
            consistency_level,
            payload,
            hash,
        }
    }

    pub fn emitter_chain_kind(&self) -> Result<ChainKind, String> {
        match self.emitter_chain {
            1 => Ok(ChainKind::Sol),
            2 | 10002 => Ok(ChainKind::Eth), // Ethereum, Sepolia
            4 => Ok(ChainKind::Bnb),
            5 | 10007 => Ok(ChainKind::Pol), // Polygon, Polygon Amoy
            22 => Ok(ChainKind::Aptos),
            23 | 10003 => Ok(ChainKind::Arb), // Arbitrum, Arbitrum Sepolia
            30 | 10004 => Ok(ChainKind::Base), // Base, Base Sepolia
            47 => Ok(ChainKind::HyperEvm),
            51 => Ok(ChainKind::Fogo),
            chain => Err(format!("Unsupported Wormhole emitter chain: {chain}")),
        }
    }
}

fn require_emitter_chain(
    address: &OmniAddress,
    emitter_chain: ChainKind,
    field: &str,
) -> Result<(), String> {
    if address.get_chain() == emitter_chain {
        Ok(())
    } else {
        Err(format!(
            "Invalid {field} chain: expected {}, got {}",
            emitter_chain.as_ref(),
            address.get_chain().as_ref()
        ))
    }
}

#[derive(Debug, BorshDeserialize)]
struct DeployTokenWh {
    payload_type: ProofKind,
    token: String,
    token_address: OmniAddress,
    decimals: u8,
    origin_decimals: u8,
}

#[derive(Debug, BorshDeserialize)]
struct LogMetadataWh {
    payload_type: ProofKind,
    token_address: OmniAddress,
    name: String,
    symbol: String,
    decimals: u8,
}

#[derive(Debug, BorshDeserialize)]
struct FinTransferWh {
    payload_type: ProofKind,
    transfer_id: TransferId,
    token_address: OmniAddress,
    amount: u128,
    fee_recipient: String,
}

#[derive(Debug, BorshDeserialize)]
struct InitTransferWh {
    payload_type: ProofKind,
    sender: OmniAddress,
    token_address: OmniAddress,
    origin_nonce: Nonce,
    amount: u128,
    fee: u128,
    native_fee: u128,
    recipient: String,
    message: String,
}

impl TryInto<InitTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<InitTransferMessage, String> {
        let transfer: InitTransferWh = borsh::from_slice(&self.payload).map_err(stringify)?;

        if transfer.payload_type != ProofKind::InitTransfer {
            return Err("Invalid proof kind".to_owned());
        }

        let emitter_chain = self.emitter_chain_kind()?;
        require_emitter_chain(&transfer.token_address, emitter_chain, "token_address")?;
        require_emitter_chain(&transfer.sender, emitter_chain, "sender")?;

        Ok(InitTransferMessage {
            token: transfer.token_address,
            amount: transfer.amount.into(),
            fee: Fee {
                fee: transfer.fee.into(),
                native_fee: transfer.native_fee.into(),
            },
            recipient: transfer.recipient.parse().map_err(stringify)?,
            origin_nonce: transfer.origin_nonce,
            sender: transfer.sender,
            msg: transfer.message,
            emitter_address: OmniAddress::new_from_slice(emitter_chain, &self.emitter_address)?,
        })
    }
}

impl TryInto<FinTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<FinTransferMessage, String> {
        let transfer: FinTransferWh = borsh::from_slice(&self.payload).map_err(stringify)?;

        if transfer.payload_type != ProofKind::FinTransfer {
            return Err("Invalid proof kind".to_owned());
        }

        let emitter_chain = self.emitter_chain_kind()?;
        require_emitter_chain(&transfer.token_address, emitter_chain, "token_address")?;

        Ok(FinTransferMessage {
            transfer_id: transfer.transfer_id,
            fee_recipient: transfer.fee_recipient.parse().ok(),
            amount: transfer.amount.into(),
            emitter_address: OmniAddress::new_from_slice(emitter_chain, &self.emitter_address)?,
        })
    }
}

impl TryInto<DeployTokenMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<DeployTokenMessage, String> {
        let parsed_payload: DeployTokenWh = borsh::from_slice(&self.payload).map_err(stringify)?;

        if parsed_payload.payload_type != ProofKind::DeployToken {
            return Err("Invalid proof kind".to_owned());
        }

        let emitter_chain = self.emitter_chain_kind()?;
        require_emitter_chain(
            &parsed_payload.token_address,
            emitter_chain,
            "token_address",
        )?;

        Ok(DeployTokenMessage {
            token: parsed_payload.token.parse().map_err(stringify)?,
            token_address: parsed_payload.token_address,
            decimals: parsed_payload.decimals,
            origin_decimals: parsed_payload.origin_decimals,
            emitter_address: OmniAddress::new_from_slice(emitter_chain, &self.emitter_address)?,
        })
    }
}

impl TryInto<LogMetadataMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<LogMetadataMessage, String> {
        let parsed_payload: LogMetadataWh = borsh::from_slice(&self.payload).map_err(stringify)?;

        if parsed_payload.payload_type != ProofKind::LogMetadata {
            return Err("Invalid proof kind".to_owned());
        }

        let emitter_chain = self.emitter_chain_kind()?;
        require_emitter_chain(
            &parsed_payload.token_address,
            emitter_chain,
            "token_address",
        )?;

        Ok(LogMetadataMessage {
            token_address: parsed_payload.token_address,
            name: parsed_payload.name,
            symbol: parsed_payload.symbol,
            decimals: parsed_payload.decimals,
            emitter_address: OmniAddress::new_from_slice(emitter_chain, &self.emitter_address)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ChainKind, ParsedVAA};

    const fn expected_ids(chain_kind: ChainKind) -> &'static [u16] {
        match chain_kind {
            ChainKind::Sol => &[1],
            ChainKind::Eth => &[2, 10002],
            ChainKind::Bnb => &[4],
            ChainKind::Pol => &[5, 10007],
            ChainKind::Aptos => &[22],
            ChainKind::Arb => &[23, 10003],
            ChainKind::Base => &[30, 10004],
            ChainKind::HyperEvm => &[47],
            ChainKind::Fogo => &[51],
            // NEAR is the bridge's destination rather than a Wormhole emitter, and Btc, Zcash,
            // Strk and Abs have no Wormhole chain id at all.
            ChainKind::Near
            | ChainKind::Btc
            | ChainKind::Zcash
            | ChainKind::Strk
            | ChainKind::Abs => &[],
        }
    }

    fn all_chain_kinds() -> impl Iterator<Item = ChainKind> {
        (0..=u8::MAX).filter_map(|discriminant| ChainKind::try_from(discriminant).ok())
    }

    fn vaa(emitter_chain: u16, payload: Vec<u8>) -> ParsedVAA {
        ParsedVAA {
            version: 1,
            guardian_set_index: 0,
            timestamp: 0,
            nonce: 0,
            len_signers: 0,
            emitter_chain,
            emitter_address: vec![0xEE; 32],
            sequence: 0,
            consistency_level: 0,
            payload,
            hash: Vec::new(),
        }
    }

    #[test]
    fn emitter_chain_kind_declared_ids_expected_chain_kinds() {
        for chain_kind in all_chain_kinds() {
            for &emitter_chain in expected_ids(chain_kind) {
                assert_eq!(
                    vaa(emitter_chain, Vec::new()).emitter_chain_kind(),
                    Ok(chain_kind)
                );
            }
        }
    }

    #[test]
    fn emitter_chain_kind_undeclared_ids_expected_error() {
        for emitter_chain in u16::MIN..=u16::MAX {
            if let Ok(chain_kind) = vaa(emitter_chain, Vec::new()).emitter_chain_kind() {
                assert!(
                    expected_ids(chain_kind).contains(&emitter_chain),
                    "{emitter_chain} unexpectedly maps to {chain_kind:?}"
                );
            }
        }
    }
}
