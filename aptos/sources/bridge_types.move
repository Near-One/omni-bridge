/// Cross-chain payload structs, events, and their Borsh encodings.
///
/// The Borsh layout below is byte-compatible with the Starknet / EVM
/// implementations of the Omni Bridge: see `starknet/src/bridge_types.cairo`
/// and `evm/src/omni-bridge/contracts/OmniBridge.sol` for reference.
module omni_bridge::bridge_types {
    use std::bcs;
    use std::option::Option;
    use std::string::String;
    use omni_bridge::borsh;

    // Payload type tags — must match the rust `PayloadType` enum on NEAR.
    const PAYLOAD_TYPE_TRANSFER_MESSAGE: u8 = 0;
    const PAYLOAD_TYPE_METADATA: u8 = 1;

    // -------- Payload structs --------

    /// Ethereum-style ECDSA signature: r || s (64 bytes) and recovery id v.
    struct Signature has copy, drop, store {
        rs: vector<u8>,
        v: u8,
    }

    /// `deploy_token` payload signed by the NEAR MPC.
    struct MetadataPayload has copy, drop, store {
        /// Source-chain token id (e.g. NEAR account id of the underlying token).
        token: String,
        name: String,
        symbol: String,
        decimals: u8,
    }

    /// `fin_transfer` payload signed by the NEAR MPC.
    struct TransferMessagePayload has copy, drop, store {
        destination_nonce: u64,
        origin_chain: u8,
        origin_nonce: u64,
        token_address: address,
        amount: u128,
        recipient: address,
        fee_recipient: Option<String>,
        message: Option<vector<u8>>,
    }

    // -------- Constructors --------

    public fun new_signature(rs: vector<u8>, v: u8): Signature {
        Signature { rs, v }
    }

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
        message: Option<vector<u8>>,
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

    public fun signature_rs(self: &Signature): vector<u8> { self.rs }
    public fun signature_v(self: &Signature): u8 { self.v }

    public fun metadata_token(self: &MetadataPayload): String { self.token }
    public fun metadata_name(self: &MetadataPayload): String { self.name }
    public fun metadata_symbol(self: &MetadataPayload): String { self.symbol }
    public fun metadata_decimals(self: &MetadataPayload): u8 { self.decimals }

    public fun transfer_destination_nonce(self: &TransferMessagePayload): u64 { self.destination_nonce }
    public fun transfer_origin_chain(self: &TransferMessagePayload): u8 { self.origin_chain }
    public fun transfer_origin_nonce(self: &TransferMessagePayload): u64 { self.origin_nonce }
    public fun transfer_token_address(self: &TransferMessagePayload): address { self.token_address }
    public fun transfer_amount(self: &TransferMessagePayload): u128 { self.amount }
    public fun transfer_recipient(self: &TransferMessagePayload): address { self.recipient }
    public fun transfer_fee_recipient(self: &TransferMessagePayload): Option<String> { self.fee_recipient }
    public fun transfer_message(self: &TransferMessagePayload): Option<vector<u8>> { self.message }

    // -------- Borsh encoding --------

    /// Borsh encoding of `MetadataPayload`, matching the Starknet / EVM
    /// `to_borsh()` implementations.
    public fun metadata_to_borsh(self: &MetadataPayload): vector<u8> {
        let buf = vector[];
        buf.push_back(PAYLOAD_TYPE_METADATA);
        buf.append(borsh::encode_string(&self.token));
        buf.append(borsh::encode_string(&self.name));
        buf.append(borsh::encode_string(&self.symbol));
        buf.push_back(self.decimals);
        buf
    }

    /// Borsh encoding of `TransferMessagePayload`, matching the Starknet /
    /// EVM `to_borsh()` implementations. The destination chain id is mixed
    /// into the hash (not the payload) to bind the signature to this chain.
    public fun transfer_message_to_borsh(
        self: &TransferMessagePayload,
        chain_id: u8,
    ): vector<u8> {
        let buf = vector[];
        buf.push_back(PAYLOAD_TYPE_TRANSFER_MESSAGE);
        buf.append(bcs::to_bytes(&self.destination_nonce));
        buf.push_back(self.origin_chain);
        buf.append(bcs::to_bytes(&self.origin_nonce));
        // OmniAddress tag for token_address: this chain.
        buf.push_back(chain_id);
        buf.append(bcs::to_bytes(&self.token_address));
        buf.append(bcs::to_bytes(&self.amount));
        // OmniAddress tag for recipient: this chain.
        buf.push_back(chain_id);
        buf.append(bcs::to_bytes(&self.recipient));

        if (self.fee_recipient.is_some()) {
            buf.push_back(1);
            let fr = *self.fee_recipient.borrow();
            buf.append(borsh::encode_string(&fr));
        } else {
            buf.push_back(0);
        };

        // Note: matches Starknet — message field is NOT wrapped in an Option
        // byte tag. None contributes nothing; Some(bytes) contributes the
        // length-prefixed bytes only.
        if (self.message.is_some()) {
            let msg = *self.message.borrow();
            buf.append(borsh::encode_byte_vec(&msg));
        };

        buf
    }

    // Events are defined in `omni_bridge::omni_bridge`. Aptos's
    // `event::emit` requires the emitter and the `#[event]` struct to live
    // in the same module, so the event definitions sit with the bridge.
}
