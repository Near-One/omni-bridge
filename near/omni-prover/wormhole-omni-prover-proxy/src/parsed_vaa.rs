//https://github.com/wormhole-foundation/wormhole/blob/main/near/contracts/wormhole/src/state.rs

use {
    crate::byte_utils::ByteUtils,
    borsh::BorshDeserialize,
    near_sdk::env,
    omni_types::{
        prover_result::{DeployTokenMessage, FinTransferMessage, InitTransferMessage},
        sol_address::SolAddress,
        stringify, EvmAddress, Fee, OmniAddress, TransferMessage, H160,
    }
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
        let len_signers = data.get_u8(Self::LEN_SIGNER_POS) as usize;
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

        ParsedVAA {
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
    token: String,
    token_address: EvmAddress,
}

#[derive(Debug, BorshDeserialize)]
struct FinTransferWh {
    _token: String,
    amount: u128,
    recipient: String,
    nonce: u128,
}

#[derive(Debug, BorshDeserialize)]
struct InitTransferWh {
    sender: EvmAddress,
    token_address: EvmAddress,
    nonce: u128,
    amount: u128,
    fee: u128,
    native_fee: u128,
    recipient: String,
    message: String,
}

impl TryInto<InitTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<InitTransferMessage, String> {
        let data: &[u8] = &self.payload[1..];
        let transfer: InitTransferWh = borsh::from_slice(data).map_err(stringify)?;

        Ok(InitTransferMessage {
            transfer: TransferMessage {
                token: to_omni_address(self.emitter_chain, &transfer.token_address.0),
                amount: transfer.amount.into(),
                fee: Fee {
                    fee: transfer.fee.into(),
                    native_fee: transfer.native_fee.into(),
                },
                recipient: transfer.recipient.parse().map_err(stringify)?,
                origin_nonce: transfer.nonce.into(),
                sender: to_omni_address(self.emitter_chain, &transfer.sender.0),
                msg: transfer.message,
            },
            emitter_address: to_omni_address(self.emitter_chain, &self.emitter_address),
        })
    }
}

impl TryInto<FinTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<FinTransferMessage, String> {
        let data: &[u8] = &self.payload[1..];
        let transfer: FinTransferWh = borsh::from_slice(data).map_err(stringify)?;

        Ok(FinTransferMessage {
            nonce: transfer.nonce.into(),
            fee_recipient: transfer.recipient.parse().map_err(stringify)?,
            amount: transfer.amount.into(),
            emitter_address: to_omni_address(self.emitter_chain, &self.emitter_address),
        })
    }
}

impl TryInto<DeployTokenMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<DeployTokenMessage, String> {
        let data: &[u8] = &self.payload[1..];
        let transfer: DeployTokenWh = borsh::from_slice(data).map_err(stringify)?;

        Ok(DeployTokenMessage {
            token: transfer.token.parse().map_err(stringify)?,
            token_address: to_omni_address(self.emitter_chain, &transfer.token_address.0),
            emitter_address: to_omni_address(self.emitter_chain, &self.emitter_address),
        })
    }
}

fn to_omni_address(emitter_chain: u16, address: &[u8]) -> OmniAddress {
    match emitter_chain {
        1 => OmniAddress::Sol(to_sol_address(address)),
        2 => OmniAddress::Eth(to_evm_address(address)),
        23 => OmniAddress::Arb(to_evm_address(address)),
        30 => OmniAddress::Base(to_evm_address(address)),
        _ => env::panic_str("Chain not supported"),
    }
}

fn to_evm_address(address: &[u8]) -> EvmAddress {
    match address.try_into() {
        Ok(bytes) => H160(bytes),
        Err(_) => env::panic_str("Invalid EVM address"),
    }
}

fn to_sol_address(address: &[u8]) -> SolAddress {
    match address.try_into() {
        Ok(bytes) => SolAddress(bytes),
        Err(_) => env::panic_str("Invalid SOL address"),
    }
}
