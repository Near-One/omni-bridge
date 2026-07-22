/// Cross-chain payload structs and their Borsh encodings.
///
/// The Borsh layout in each `*_to_borsh` is byte-compatible with the
/// Aptos / Starknet / EVM siblings: see `aptos/sources/bridge_types.move`,
/// `starknet/src/bridge_types.cairo` and
/// `evm/src/omni-bridge/contracts/OmniBridge.sol`. The only difference on
/// Sui is what the 32-byte `token_address` denotes: the keccak256 hash of
/// the coin's canonical type string (see `utils::token_address_bytes`),
/// since Sui coins are types, not addresses.
///
/// Signatures are passed around as a single 65-byte `r || s || v` vector —
/// exactly what the NEAR MPC emits.
module omni_bridge::bridge_types;

use omni_bridge::borsh;
use std::bcs;
use std::string::String;

// Payload type tags — must match the rust `PayloadType` enum on NEAR.
const PAYLOAD_TYPE_TRANSFER_MESSAGE: u8 = 0;
const PAYLOAD_TYPE_METADATA: u8 = 1;

/// `deploy_token` payload signed by the NEAR MPC.
public struct MetadataPayload has copy, drop {
    /// NEAR token account id of the token being deployed.
    token: String,
    name: String,
    symbol: String,
    decimals: u8,
}

/// `fin_transfer` payload signed by the NEAR MPC.
public struct TransferMessagePayload has copy, drop {
    destination_nonce: u64,
    origin_chain: u8,
    origin_nonce: u64,
    /// keccak256 of the coin's canonical type string.
    token_address: address,
    amount: u128,
    recipient: address,
    fee_recipient: Option<String>,
    /// Empty vector == no message (NEAR never signs `Some(empty)`).
    message: vector<u8>,
}

// -------- Constructors --------

public fun new_metadata_payload(
    token: String,
    name: String,
    symbol: String,
    decimals: u8,
): MetadataPayload {
    MetadataPayload { token, name, symbol, decimals }
}

public fun new_transfer_message_payload(
    destination_nonce: u64,
    origin_chain: u8,
    origin_nonce: u64,
    token_address: address,
    amount: u128,
    recipient: address,
    fee_recipient: Option<String>,
    message: vector<u8>,
): TransferMessagePayload {
    TransferMessagePayload {
        destination_nonce,
        origin_chain,
        origin_nonce,
        token_address,
        amount,
        recipient,
        fee_recipient,
        message,
    }
}

// -------- Accessors --------

public fun metadata_token(self: &MetadataPayload): String {
    self.token
}

public fun metadata_name(self: &MetadataPayload): String {
    self.name
}

public fun metadata_symbol(self: &MetadataPayload): String {
    self.symbol
}

public fun metadata_decimals(self: &MetadataPayload): u8 {
    self.decimals
}

public fun transfer_fee_recipient(self: &TransferMessagePayload): Option<String> {
    self.fee_recipient
}

public fun transfer_message(self: &TransferMessagePayload): vector<u8> {
    self.message
}

// -------- Borsh encoding --------

/// Borsh encoding of `MetadataPayload`. Byte-identical to Aptos / Starknet
/// / EVM.
public fun metadata_to_borsh(self: &MetadataPayload): vector<u8> {
    let mut buf = vector[PAYLOAD_TYPE_METADATA];
    buf.append(borsh::encode_string(&self.token));
    buf.append(borsh::encode_string(&self.name));
    buf.append(borsh::encode_string(&self.symbol));
    buf.push_back(self.decimals);
    buf
}

/// Borsh encoding of `TransferMessagePayload`. Byte-identical to the
/// sibling chains. `chain_id` is interleaved as the OmniAddress tag before
/// each of `token_address` and `recipient` and is bound into the signed
/// hash (not the payload), preventing cross-chain replay.
public fun transfer_message_to_borsh(
    self: &TransferMessagePayload,
    chain_id: u8,
): vector<u8> {
    let mut buf = vector[PAYLOAD_TYPE_TRANSFER_MESSAGE];
    buf.append(bcs::to_bytes(&self.destination_nonce));
    buf.push_back(self.origin_chain);
    buf.append(bcs::to_bytes(&self.origin_nonce));
    buf.push_back(chain_id);
    buf.append(bcs::to_bytes(&self.token_address));
    buf.append(bcs::to_bytes(&self.amount));
    buf.push_back(chain_id);
    buf.append(bcs::to_bytes(&self.recipient));

    if (self.fee_recipient.is_some()) {
        buf.push_back(1);
        buf.append(borsh::encode_string(self.fee_recipient.borrow()));
    } else {
        buf.push_back(0);
    };

    // Note: matches Aptos / Starknet — `message` is NOT wrapped in an
    // Option byte tag. Empty contributes nothing; non-empty contributes
    // only the length-prefixed bytes.
    if (!self.message.is_empty()) {
        buf.append(borsh::encode_byte_vec(&self.message));
    };

    buf
}
