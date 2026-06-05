/// Cross-chain payload structs and their Borsh encodings.
///
/// The Borsh layout in each `*_to_borsh` is byte-compatible with the
/// Starknet / EVM siblings: see `starknet/src/bridge_types.cairo` and
/// `evm/src/omni-bridge/contracts/OmniBridge.sol`. Move doesn't allow
/// overloading function names by receiver type within a module, so the
/// two `*_to_borsh` helpers carry the payload-type prefix; both are still
/// callable via receiver syntax (`payload.metadata_to_borsh()`).
///
/// Signatures (`r || s`, `v`) are passed around as plain `(vector<u8>, u8)`
/// rather than a wrapper struct — the struct added no invariant.
module omni_bridge::bridge_types {
    use std::bcs;
    use std::option::Option;
    use std::string::String;
    use omni_bridge::borsh;

    // Payload type tags — must match the rust `PayloadType` enum on NEAR.
    const PAYLOAD_TYPE_TRANSFER_MESSAGE: u8 = 0;
    const PAYLOAD_TYPE_METADATA: u8 = 1;

    /// `deploy_token` payload signed by the NEAR MPC.
    struct MetadataPayload has copy, drop, store {
        /// Source-chain token id (e.g. NEAR account id of the underlying token).
        token: String,
        name: String,
        symbol: String,
        decimals: u8
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
        message: Option<vector<u8>>
    }

    // -------- Constructors --------

    public fun new_metadata_payload(
        token: String,
        name: String,
        symbol: String,
        decimals: u8
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
        message: Option<vector<u8>>
    ): TransferMessagePayload {
        TransferMessagePayload {
            destination_nonce,
            origin_chain,
            origin_nonce,
            token_address,
            amount,
            recipient,
            fee_recipient,
            message
        }
    }

    // -------- Accessors --------

    public fun metadata_token(self: &MetadataPayload): String {
        self.token
    }
    public fun metadata_name(self: &MetadataPayload): String
 {
        self.name
    }
    public fun metadata_symbol(self: &MetadataPayload): String
 {
        self.symbol
    }
    public fun metadata_decimals(self: &MetadataPayload): u8
 {
        self.decimals
    }

    public fun transfer_fee_recipient(self: &TransferMessagePayload): Option<String> {
        self.fee_recipient
    }
    public fun transfer_message(self: &TransferMessagePayload): Option<vector<u8>>
 {
        self.message
    }

    // -------- Borsh encoding --------

    /// Borsh encoding of `MetadataPayload`. Byte-identical to Starknet / EVM.
    public fun metadata_to_borsh(self: &MetadataPayload): vector<u8> {
        let buf = vector[];
        buf.push_back(PAYLOAD_TYPE_METADATA);
        buf.append(borsh::encode_string(&self.token));
        buf.append(borsh::encode_string(&self.name));
        buf.append(borsh::encode_string(&self.symbol));
        buf.push_back(self.decimals);
        buf
    }

    /// Borsh encoding of `TransferMessagePayload`. Byte-identical to
    /// Starknet / EVM. `chain_id` is interleaved as the OmniAddress tag
    /// before each of `token_address` and `recipient` and is bound into
    /// the signed hash (not the payload), preventing cross-chain replay.
    public fun transfer_message_to_borsh(
        self: &TransferMessagePayload, chain_id: u8
    ): vector<u8> {
        let buf = vector[];
        buf.push_back(PAYLOAD_TYPE_TRANSFER_MESSAGE);
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
            let fr = *self.fee_recipient.borrow();
            buf.append(borsh::encode_string(&fr));
        } else {
            buf.push_back(0);
        };

        // Note: matches Starknet — `message` is NOT wrapped in an Option
        // byte tag. None contributes nothing; Some(bytes) contributes only
        // the length-prefixed bytes.
        if (self.message.is_some()) {
            let msg = *self.message.borrow();
            buf.append(borsh::encode_byte_vec(&msg));
        };

        buf
    }
}

