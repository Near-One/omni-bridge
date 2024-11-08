//https://github.com/wormhole-foundation/wormhole/blob/main/near/contracts/wormhole/src/state.rs

use {
    crate::byte_utils::ByteUtils,
    alloy_sol_types::{sol, SolType},
    near_sdk::env,
    omni_types::{
        prover_result::{DeployTokenMessage, FinTransferMessage, InitTransferMessage},
        sol_address::SolAddress,
        stringify, EvmAddress, Fee, OmniAddress, TransferMessage, H160,
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

sol! {
    struct InitTransferWh {
        uint8 messageType;
        address sender;
        address tokenAddress;
        uint128 nonce;
        uint128 amount;
        uint128 fee;
        uint128 nativeFee;
        string recipient;
        string message;
    }

    struct FinTransferWh {
        uint8 messageType;
        string token;
        uint128 amount;
        string recipient;
        uint128 nonce;
    }

    struct DeployTokenWh {
        uint8 messageType;
        string token;
        address tokenAddress;
    }
}

impl TryInto<InitTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<InitTransferMessage, String> {
        let transfer =
            InitTransferWh::abi_decode_sequence(&self.payload, true).map_err(stringify)?;
        Ok(InitTransferMessage {
            transfer: TransferMessage {
                token: to_omni_address(self.emitter_chain, &transfer.tokenAddress.0 .0),
                amount: transfer.amount.into(),
                fee: Fee {
                    fee: transfer.fee.into(),
                    native_fee: transfer.nativeFee.into(),
                },
                recipient: transfer.recipient.parse().map_err(stringify)?,
                origin_nonce: transfer.nonce.into(),
                sender: to_omni_address(self.emitter_chain, &transfer.sender.0 .0),
                msg: transfer.message,
            },
            emitter_address: to_omni_address(self.emitter_chain, &self.emitter_address),
        })
    }
}

impl TryInto<FinTransferMessage> for ParsedVAA {
    type Error = String;

    fn try_into(self) -> Result<FinTransferMessage, String> {
        let transfer =
            FinTransferWh::abi_decode_sequence(&self.payload, true).map_err(stringify)?;

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
        let transfer =
            DeployTokenWh::abi_decode_sequence(&self.payload, true).map_err(stringify)?;

        Ok(DeployTokenMessage {
            token: transfer.token.parse().map_err(stringify)?,
            token_address: to_omni_address(self.emitter_chain, &transfer.tokenAddress.0 .0),
            emitter_address: to_omni_address(self.emitter_chain, &self.emitter_address),
        })
    }
}

fn to_omni_address(emitter_chain: u16, address: &[u8]) -> OmniAddress {
    match emitter_chain {
        1 => OmniAddress::Sol(to_sol_address(address)),
        2 | 10002 => OmniAddress::Eth(to_evm_address(address)),
        23 | 10003 => OmniAddress::Arb(to_evm_address(address)),
        30 | 10004 => OmniAddress::Base(to_evm_address(address)),
        _ => env::panic_str("Chain not supported"),
    }
}

fn to_evm_address(address: &[u8]) -> EvmAddress {
    let address = if address.len() == 32 {
        &address[address.len() - 20..]
    } else {
        address
    };

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
