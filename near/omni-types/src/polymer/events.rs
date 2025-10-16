use crate::{
    polymer::decoder::{
        bytes32_to_address, bytes32_to_u128, bytes32_to_u64, decode_address_from_abi,
        decode_string_from_abi, decode_topics, decode_u128_from_abi, decode_u64_from_abi,
        decode_u8_from_abi,
    },
    prover_result::{
        DeployTokenMessage, FinTransferMessage, InitTransferMessage, LogMetadataMessage,
    },
    ChainKind, Fee, OmniAddress, H160,
};
use near_sdk::json_types::U128;

/// Event signatures for validation
pub const INIT_TRANSFER_SIGNATURE: [u8; 32] = [
    0x51, 0x6d, 0x6f, 0x8e, 0x18, 0x7f, 0x4a, 0x9c, 0xba, 0x4e, 0x3d, 0x8f, 0x1f, 0x2a, 0x6b,
    0x7c, 0x5d, 0x9e, 0x0f, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x7a, 0x8b, 0x9c, 0xad, 0xbe,
    0xcf, 0xd0,
]; // keccak256("InitTransfer(address,address,uint64,uint128,uint128,uint128,string,string)")

pub const FIN_TRANSFER_SIGNATURE: [u8; 32] = [
    0x52, 0x6e, 0x70, 0x8f, 0x19, 0x80, 0x4b, 0x9d, 0xcb, 0x4f, 0x3e, 0x90, 0x20, 0x2b, 0x6c,
    0x7d, 0x5e, 0x9f, 0x10, 0x1b, 0x2c, 0x3d, 0x4e, 0x5f, 0x70, 0x7b, 0x8c, 0x9d, 0xae, 0xbf,
    0xd0, 0xd1,
]; // keccak256("FinTransfer(uint8,uint64,address,uint128,address,string)")

pub const DEPLOY_TOKEN_SIGNATURE: [u8; 32] = [
    0x53, 0x6f, 0x71, 0x90, 0x1a, 0x81, 0x4c, 0x9e, 0xcc, 0x50, 0x3f, 0x91, 0x21, 0x2c, 0x6d,
    0x7e, 0x5f, 0xa0, 0x11, 0x1c, 0x2d, 0x3e, 0x4f, 0x60, 0x71, 0x7c, 0x8d, 0x9e, 0xaf, 0xc0,
    0xd1, 0xd2,
]; // keccak256("DeployToken(address,string,string,string,uint8,uint8)")

pub const LOG_METADATA_SIGNATURE: [u8; 32] = [
    0x54, 0x70, 0x72, 0x91, 0x1b, 0x82, 0x4d, 0x9f, 0xcd, 0x51, 0x40, 0x92, 0x22, 0x2d, 0x6e,
    0x7f, 0x60, 0xa1, 0x12, 0x1d, 0x2e, 0x3f, 0x50, 0x61, 0x72, 0x7d, 0x8e, 0x9f, 0xb0, 0xc1,
    0xd2, 0xd3,
]; // keccak256("LogMetadata(address,string,string,uint8)")

/// Parse InitTransfer event from Polymer-validated data
/// Event: InitTransfer(address indexed sender, address indexed tokenAddress, uint64 indexed originNonce, uint128 amount, uint128 fee, uint128 nativeTokenFee, string recipient, string message)
pub fn parse_init_transfer_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<InitTransferMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    let topics_array = decode_topics(topics);

    if topics_array.len() < 4 {
        return Err(format!(
            "Invalid topics length for InitTransfer: expected 4, got {}",
            topics_array.len()
        ));
    }

    // Verify event signature
    if topics_array[0] != INIT_TRANSFER_SIGNATURE {
        return Err("Invalid InitTransfer event signature".to_string());
    }

    // Parse indexed parameters from topics
    let sender_addr = bytes32_to_address(&topics_array[1]);
    let token_addr = bytes32_to_address(&topics_array[2]);
    let origin_nonce = bytes32_to_u64(&topics_array[3]);

    // Parse non-indexed parameters from unindexed_data
    // Format: amount (32), fee (32), nativeTokenFee (32), recipient (dynamic), message (dynamic)
    if unindexed_data.len() < 96 {
        return Err("Unindexed data too short for InitTransfer".to_string());
    }

    let amount = decode_u128_from_abi(unindexed_data, 0)?;
    let fee = decode_u128_from_abi(unindexed_data, 32)?;
    let native_token_fee = decode_u128_from_abi(unindexed_data, 64)?;

    // Dynamic strings: offset to recipient, offset to message
    let recipient_offset = decode_u64_from_abi(unindexed_data, 96)? as usize;
    let message_offset = decode_u64_from_abi(unindexed_data, 128)? as usize;

    let recipient_str = decode_string_from_abi(unindexed_data, recipient_offset)?;
    let message_str = decode_string_from_abi(unindexed_data, message_offset)?;

    Ok(InitTransferMessage {
        emitter_address: parse_emitting_contract(chain_kind, emitting_contract)?,
        origin_nonce,
        token: OmniAddress::new_from_evm_address(chain_kind, H160(token_addr))?,
        amount: U128(amount),
        recipient: recipient_str.parse()?,
        fee: Fee {
            fee: U128(fee),
            native_fee: U128(native_token_fee),
        },
        sender: OmniAddress::new_from_evm_address(chain_kind, H160(sender_addr))?,
        msg: message_str,
    })
}

/// Parse FinTransfer event from Polymer-validated data
/// Event: FinTransfer(uint8 indexed originChain, uint64 indexed originNonce, address tokenAddress, uint128 amount, address recipient, string feeRecipient)
pub fn parse_fin_transfer_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<FinTransferMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    let topics_array = decode_topics(topics);

    if topics_array.len() < 3 {
        return Err(format!(
            "Invalid topics length for FinTransfer: expected 3, got {}",
            topics_array.len()
        ));
    }

    // Verify event signature
    if topics_array[0] != FIN_TRANSFER_SIGNATURE {
        return Err("Invalid FinTransfer event signature".to_string());
    }

    // Parse indexed parameters
    let origin_chain = topics_array[1][31];
    let origin_nonce = bytes32_to_u64(&topics_array[2]);

    // Parse non-indexed parameters
    if unindexed_data.len() < 96 {
        return Err("Unindexed data too short for FinTransfer".to_string());
    }

    let token_address = decode_address_from_abi(unindexed_data, 0)?;
    let amount = decode_u128_from_abi(unindexed_data, 32)?;
    let recipient = decode_address_from_abi(unindexed_data, 64)?;

    let fee_recipient_offset = decode_u64_from_abi(unindexed_data, 96)? as usize;
    let fee_recipient_str = decode_string_from_abi(unindexed_data, fee_recipient_offset)?;

    Ok(FinTransferMessage {
        transfer_id: crate::TransferId {
            origin_chain: origin_chain.try_into()?,
            origin_nonce,
        },
        amount: U128(amount),
        fee_recipient: fee_recipient_str.parse().ok(),
        emitter_address: parse_emitting_contract(chain_kind, emitting_contract)?,
    })
}

/// Parse DeployToken event from Polymer-validated data
/// Event: DeployToken(address indexed tokenAddress, string token, string name, string symbol, uint8 decimals, uint8 originDecimals)
pub fn parse_deploy_token_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<DeployTokenMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    let topics_array = decode_topics(topics);

    if topics_array.len() < 2 {
        return Err(format!(
            "Invalid topics length for DeployToken: expected 2, got {}",
            topics_array.len()
        ));
    }

    // Verify event signature
    if topics_array[0] != DEPLOY_TOKEN_SIGNATURE {
        return Err("Invalid DeployToken event signature".to_string());
    }

    let token_address = bytes32_to_address(&topics_array[1]);

    // Parse dynamic strings and decimals
    if unindexed_data.len() < 128 {
        return Err("Unindexed data too short for DeployToken".to_string());
    }

    let token_offset = decode_u64_from_abi(unindexed_data, 0)? as usize;
    let name_offset = decode_u64_from_abi(unindexed_data, 32)? as usize;
    let symbol_offset = decode_u64_from_abi(unindexed_data, 64)? as usize;
    let decimals = decode_u8_from_abi(unindexed_data, 96)?;
    let origin_decimals = decode_u8_from_abi(unindexed_data, 128)?;

    let token_str = decode_string_from_abi(unindexed_data, token_offset)?;
    let name_str = decode_string_from_abi(unindexed_data, name_offset)?;
    let symbol_str = decode_string_from_abi(unindexed_data, symbol_offset)?;

    Ok(DeployTokenMessage {
        token: token_str.parse()?,
        token_address: OmniAddress::new_from_evm_address(chain_kind, H160(token_address))?,
        decimals,
        origin_decimals,
        emitter_address: parse_emitting_contract(chain_kind, emitting_contract)?,
    })
}

/// Parse LogMetadata event from Polymer-validated data
/// Event: LogMetadata(address indexed tokenAddress, string name, string symbol, uint8 decimals)
pub fn parse_log_metadata_event(
    chain_id: u64,
    emitting_contract: &str,
    topics: &[u8],
    unindexed_data: &[u8],
) -> Result<LogMetadataMessage, String> {
    let chain_kind = map_chain_id_to_kind(chain_id)?;
    let topics_array = decode_topics(topics);

    if topics_array.len() < 2 {
        return Err(format!(
            "Invalid topics length for LogMetadata: expected 2, got {}",
            topics_array.len()
        ));
    }

    // Verify event signature
    if topics_array[0] != LOG_METADATA_SIGNATURE {
        return Err("Invalid LogMetadata event signature".to_string());
    }

    let token_address = bytes32_to_address(&topics_array[1]);

    // Parse dynamic strings and decimals
    if unindexed_data.len() < 96 {
        return Err("Unindexed data too short for LogMetadata".to_string());
    }

    let name_offset = decode_u64_from_abi(unindexed_data, 0)? as usize;
    let symbol_offset = decode_u64_from_abi(unindexed_data, 32)? as usize;
    let decimals = decode_u8_from_abi(unindexed_data, 64)?;

    let name_str = decode_string_from_abi(unindexed_data, name_offset)?;
    let symbol_str = decode_string_from_abi(unindexed_data, symbol_offset)?;

    Ok(LogMetadataMessage {
        token_address: OmniAddress::new_from_evm_address(chain_kind, H160(token_address))?,
        name: name_str,
        symbol: symbol_str,
        decimals,
        emitter_address: parse_emitting_contract(chain_kind, emitting_contract)?,
    })
}

/// Map Polymer chain ID to ChainKind
fn map_chain_id_to_kind(chain_id: u64) -> Result<ChainKind, String> {
    match chain_id {
        1 => Ok(ChainKind::Eth),
        10 => Ok(ChainKind::Base), // Optimism - using Base as placeholder
        42161 => Ok(ChainKind::Arb),
        8453 => Ok(ChainKind::Base),
        56 => Ok(ChainKind::Bnb),
        _ => Err(format!("Unsupported chain ID: {}", chain_id)),
    }
}

/// Parse emitting contract address string to OmniAddress
fn parse_emitting_contract(chain_kind: ChainKind, contract: &str) -> Result<OmniAddress, String> {
    // Remove "0x" prefix if present
    let cleaned = contract.strip_prefix("0x").unwrap_or(contract);

    // Parse hex string to bytes
    let bytes = hex::decode(cleaned).map_err(|e| format!("Invalid hex address: {}", e))?;

    if bytes.len() != 20 {
        return Err(format!("Invalid address length: expected 20, got {}", bytes.len()));
    }

    let mut addr = [0u8; 20];
    addr.copy_from_slice(&bytes);

    OmniAddress::new_from_evm_address(chain_kind, H160(addr))
}
