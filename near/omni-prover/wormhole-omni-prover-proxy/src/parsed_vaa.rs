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
        stringify, Fee, Nonce, OmniAddress, TransferId,
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

        Ok(InitTransferMessage {
            token: transfer.token_address.clone(),
            amount: transfer.amount.into(),
            fee: Fee {
                fee: transfer.fee.into(),
                native_fee: transfer.native_fee.into(),
            },
            recipient: transfer.recipient.parse().map_err(stringify)?,
            origin_nonce: transfer.origin_nonce,
            sender: transfer.sender,
            msg: transfer.message,
            emitter_address: OmniAddress::new_from_slice(
                transfer.token_address.get_chain(),
                &self.emitter_address,
            )?,
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

        Ok(FinTransferMessage {
            transfer_id: transfer.transfer_id,
            fee_recipient: transfer.fee_recipient.parse().ok(),
            amount: transfer.amount.into(),
            emitter_address: OmniAddress::new_from_slice(
                transfer.token_address.get_chain(),
                &self.emitter_address,
            )?,
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

        Ok(DeployTokenMessage {
            token: parsed_payload.token.parse().map_err(stringify)?,
            token_address: parsed_payload.token_address.clone(),
            decimals: parsed_payload.decimals,
            origin_decimals: parsed_payload.origin_decimals,
            emitter_address: OmniAddress::new_from_slice(
                parsed_payload.token_address.get_chain(),
                &self.emitter_address,
            )?,
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

        let chain_kind = parsed_payload.token_address.get_chain();
        Ok(LogMetadataMessage {
            token_address: parsed_payload.token_address,
            name: parsed_payload.name,
            symbol: parsed_payload.symbol,
            decimals: parsed_payload.decimals,
            emitter_address: OmniAddress::new_from_slice(chain_kind, &self.emitter_address)?,
        })
    }
}
