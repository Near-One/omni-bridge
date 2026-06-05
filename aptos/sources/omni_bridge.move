/// Aptos side of the NEAR Omni Bridge.
///
/// Cross-chain bridge contract enabling token transfers between Aptos and
/// other chains via NEAR Protocol. All transfers route through NEAR
/// (Aptos ↔ NEAR ↔ other chain). Security is rooted in Ethereum-style
/// ECDSA signatures by the NEAR MPC, verified against
/// `near_bridge_derived_address`.
///
/// See [starknet/src/omni_bridge.cairo] and
/// [evm/src/omni-bridge/contracts/OmniBridge.sol] for the sibling
/// implementations whose payload encodings this module mirrors.
module omni_bridge::omni_bridge {
    use std::string::String;
    use std::option::{Self, Option};
    use aptos_std::aptos_hash;
    use aptos_std::table::{Self, Table};
    use aptos_framework::event;
    use aptos_framework::fungible_asset::{Self, Metadata};
    use aptos_framework::object::{Self, ExtendRef, Object};
    use aptos_framework::primary_fungible_store;

    use omni_bridge::bridge_token;
    use omni_bridge::bridge_types;
    use omni_bridge::utils;

    // -------- Errors --------
    // Numeric values are part of the test contract — never reorder.
    // Grouped: lifecycle → auth → pause → deploy → transfer.

    // Lifecycle / auth
    const E_ALREADY_INITIALIZED: u64 = 1;
    const E_NOT_ADMIN: u64 = 2;
    const E_NOT_PAUSER: u64 = 3;

    // Pause flags
    const E_INIT_TRANSFER_PAUSED: u64 = 4;
    const E_FIN_TRANSFER_PAUSED: u64 = 5;
    const E_DEPLOY_TOKEN_PAUSED: u64 = 6;

    // Deploy token
    const E_TOKEN_ALREADY_DEPLOYED: u64 = 7;

    // Transfer
    const E_NONCE_ALREADY_USED: u64 = 8;
    const E_ZERO_AMOUNT: u64 = 9;
    const E_INVALID_FEE: u64 = 10;
    const E_AMOUNT_OVERFLOW: u64 = 11;

    /// Largest amount that fits in `u64`, used to bound `u128` payload
    /// amounts before they're handed to the Aptos Fungible Asset APIs.
    const MAX_U64_AS_U128: u128 = 0xFFFFFFFFFFFFFFFF;

    // -------- Pause flags --------

    const PAUSE_INIT_TRANSFER: u8 = 0x01;
    const PAUSE_FIN_TRANSFER: u8 = 0x02;
    const PAUSE_DEPLOY_TOKEN: u8 = 0x04;
    const PAUSE_ALL: u8 = 0xFF;

    /// Seed used to create the deterministic bridge state object under
    /// `@omni_bridge`. The state lives at `object::create_object_address(
    /// &@omni_bridge, BRIDGE_OBJECT_SEED)`.
    const BRIDGE_OBJECT_SEED: vector<u8> = b"omni_bridge::state";

    /// Bitmap word width — keep at 128 so each entry packs nonces [n*128, n*128+127].
    const BITMAP_WIDTH: u64 = 128;

    /// Top-level bridge state. Stored as a resource on the bridge object
    /// (a named Object owned by `@omni_bridge`).
    struct BridgeState has key {
        /// Address authorized to administer the bridge (set pause flags,
        /// rotate keys, etc.).
        admin: address,
        /// Address authorized to call `pause_all`. Separate from admin so
        /// the pauser key can be operationally hot without granting full
        /// admin rights.
        pauser: address,
        /// Bitfield of paused operations. See `PAUSE_*` constants.
        pause_flags: u8,
        /// 20-byte recovered address of the NEAR MPC-derived Ethereum signer.
        near_bridge_derived_address: vector<u8>,
        /// Chain id of this bridge instance (mixed into transfer payload hashes
        /// to prevent cross-chain replay).
        chain_id: u8,
        /// Monotonically increasing origin nonce assigned to outbound transfers.
        current_origin_nonce: u64,
        /// Bitmap of finalized destination nonces. `slot = nonce / 128`,
        /// `bit = nonce % 128`. Persists in a `Table<u64, u128>`.
        completed_transfers: Table<u64, u128>,
        /// FA metadata for the chain native token used for native fees (APT FA).
        native_token_metadata: Object<Metadata>,
        /// keccak(near_token_id) → FA metadata object address.
        near_to_aptos_token: Table<vector<u8>, address>,
        /// ExtendRef for the bridge object. Used to derive the object's signer
        /// on demand for:
        ///   - creating new FA objects in `deploy_token`
        ///   - moving locked tokens out in `fin_transfer` (non-bridge tokens)
        extend_ref: ExtendRef
    }

    // -------- Events --------
    // Aptos requires `#[event]` structs and `event::emit` to live in the
    // same module, so the events sit here next to the bridge logic that
    // emits them.

    #[event]
    struct LogMetadata has drop, store {
        token_address: address,
        name: String,
        symbol: String,
        decimals: u8
    }

    #[event]
    struct DeployToken has drop, store {
        token_address: address,
        near_token_id: String,
        name: String,
        symbol: String,
        decimals: u8,
        origin_decimals: u8
    }

    #[event]
    struct InitTransfer has drop, store {
        sender: address,
        token_address: address,
        origin_nonce: u64,
        amount: u128,
        fee: u128,
        native_fee: u128,
        recipient: String,
        message: vector<u8>
    }

    #[event]
    struct FinTransfer has drop, store {
        origin_chain: u8,
        origin_nonce: u64,
        token_address: address,
        amount: u128,
        recipient: address,
        fee_recipient: Option<String>,
        message: Option<vector<u8>>
    }

    #[event]
    struct PauseStateChanged has drop, store {
        old_flags: u8,
        new_flags: u8,
        admin: address
    }

    // -------- Initialization --------

    /// Initialize the bridge. Callable exactly once by the module deployer.
    ///
    /// Creates a deterministic named Object owned by `@omni_bridge` and
    /// stores `BridgeState` on that object. The object holds the bridge's
    /// `ExtendRef`, used later to derive a signer for creating bridged FA
    /// tokens and custodying locked tokens.
    public entry fun initialize(
        deployer: &signer,
        near_bridge_derived_address: vector<u8>,
        chain_id: u8,
        native_token_metadata: Object<Metadata>
    ) {
        let deployer_addr = deployer.address_of();
        assert!(deployer_addr == @omni_bridge, E_NOT_ADMIN);
        assert!(
            !exists<BridgeState>(bridge_object_address()),
            E_ALREADY_INITIALIZED
        );

        let constructor_ref = object::create_named_object(deployer, BRIDGE_OBJECT_SEED);
        let extend_ref = constructor_ref.generate_extend_ref();
        // Permanently pin the bridge object at `bridge_object_address()` so a
        // compromise of the deployer key cannot move locked tokens elsewhere.
        let transfer_ref = constructor_ref.generate_transfer_ref();
        transfer_ref.disable_ungated_transfer();
        let object_signer = constructor_ref.generate_signer();

        move_to(
            &object_signer,
            BridgeState {
                admin: deployer_addr,
                pauser: deployer_addr,
                pause_flags: 0,
                near_bridge_derived_address,
                chain_id,
                current_origin_nonce: 0,
                completed_transfers: table::new<u64, u128>(),
                native_token_metadata,
                near_to_aptos_token: table::new<vector<u8>, address>(),
                extend_ref
            }
        );
    }

    #[view]
    /// Deterministic address where `BridgeState` lives. Locked tokens are
    /// custodied here for the native-FA pass-through path.
    public fun bridge_object_address(): address {
        object::create_object_address(&@omni_bridge, BRIDGE_OBJECT_SEED)
    }

    // -------- Admin --------

    public entry fun set_admin(admin: &signer, new_admin: address) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_admin(state, admin);
        state.admin = new_admin;
    }

    public entry fun set_pauser(admin: &signer, new_pauser: address) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_admin(state, admin);
        state.pauser = new_pauser;
    }

    public entry fun set_near_bridge_derived_address(
        admin: &signer, new_address: vector<u8>
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_admin(state, admin);
        state.near_bridge_derived_address = new_address;
    }

    public entry fun set_pause_flags(admin: &signer, flags: u8) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_admin(state, admin);
        let old = state.pause_flags;
        state.pause_flags = flags;
        event::emit(
            PauseStateChanged {
                old_flags: old,
                new_flags: flags,
                admin: admin.address_of()
            }
        );
    }

    public entry fun pause_all(pauser: &signer) {
        let state = &mut BridgeState[bridge_object_address()];
        assert!(pauser.address_of() == state.pauser, E_NOT_PAUSER);
        let old = state.pause_flags;
        state.pause_flags = PAUSE_ALL;
        event::emit(
            PauseStateChanged {
                old_flags: old,
                new_flags: PAUSE_ALL,
                admin: pauser.address_of()
            }
        );
    }

    // -------- Token discovery --------

    /// Permissionless: emit a `LogMetadata` event describing an existing FA.
    /// The NEAR side picks this event up to decide whether to sign a
    /// `deploy_token` payload for the mirror token on its side.
    public entry fun log_metadata(token: Object<Metadata>) {
        let name = fungible_asset::name(token);
        let symbol = fungible_asset::symbol(token);
        let decimals = fungible_asset::decimals(token);
        event::emit(
            LogMetadata { token_address: token.object_address(), name, symbol, decimals }
        );
    }

    // -------- Bridge operations --------

    /// Deploy a bridged FA token. Anyone may submit the transaction —
    /// security comes from the NEAR MPC signature over the payload, not
    /// access control. The transaction signer is not read on-chain.
    public entry fun deploy_token(
        signature_rs: vector<u8>,
        signature_v: u8,
        token: String,
        name: String,
        symbol: String,
        decimals: u8
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert!(
            (state.pause_flags & PAUSE_DEPLOY_TOKEN) == 0,
            E_DEPLOY_TOKEN_PAUSED
        );

        let payload = bridge_types::new_metadata_payload(token, name, symbol, decimals);
        let encoded = payload.metadata_to_borsh();
        verify_signature(state, encoded, signature_rs, signature_v);

        let token_id_hash = aptos_hash::keccak256(*payload.metadata_token().bytes());
        assert!(
            !state.near_to_aptos_token.contains(token_id_hash),
            E_TOKEN_ALREADY_DEPLOYED
        );

        let normalized_decimals = utils::normalize_decimals(payload.metadata_decimals());

        let resource_signer = state.extend_ref.generate_signer_for_extending();
        let metadata =
            bridge_token::create(
                &resource_signer,
                token_id_hash,
                payload.metadata_name(),
                payload.metadata_symbol(),
                normalized_decimals
            );

        let token_addr = metadata.object_address();
        state.near_to_aptos_token.add(token_id_hash, token_addr);
        // No reverse-direction table: bridge-token status is determined by
        // the presence of `BridgeTokenRefs` on the FA object itself (see
        // `bridge_token::is_bridge_token`). One source of truth.

        event::emit(
            DeployToken {
                token_address: token_addr,
                near_token_id: payload.metadata_token(),
                name: payload.metadata_name(),
                symbol: payload.metadata_symbol(),
                decimals: normalized_decimals,
                origin_decimals: payload.metadata_decimals()
            }
        );
    }

    /// Finalize an inbound transfer from another chain. Permissionless —
    /// the NEAR MPC signature is the authorization. The transaction signer
    /// is not read on-chain.
    public entry fun fin_transfer(
        signature_rs: vector<u8>,
        signature_v: u8,
        destination_nonce: u64,
        origin_chain: u8,
        origin_nonce: u64,
        token_address: address,
        amount: u128,
        recipient: address,
        fee_recipient: Option<String>,
        message: Option<vector<u8>>
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert!(
            (state.pause_flags & PAUSE_FIN_TRANSFER) == 0,
            E_FIN_TRANSFER_PAUSED
        );

        assert!(
            !is_nonce_used(&state.completed_transfers, destination_nonce),
            E_NONCE_ALREADY_USED
        );
        mark_nonce_used(&mut state.completed_transfers, destination_nonce);

        let payload =
            bridge_types::new_transfer_message_payload(
                destination_nonce,
                origin_chain,
                origin_nonce,
                token_address,
                amount,
                recipient,
                fee_recipient,
                message
            );
        let encoded = payload.transfer_message_to_borsh(state.chain_id);
        verify_signature(state, encoded, signature_rs, signature_v);

        // Cap to u64 — Aptos FA amounts are u64.
        assert!(amount <= MAX_U64_AS_U128, E_AMOUNT_OVERFLOW);
        let amount_u64 = (amount as u64);

        let metadata = object::address_to_object<Metadata>(token_address);
        if (bridge_token::is_bridge_token(metadata)) {
            bridge_token::mint(metadata, recipient, amount_u64);
        } else {
            // Locked-token path: bridge resource account custodies the supply.
            let resource_signer = state.extend_ref.generate_signer_for_extending();
            primary_fungible_store::transfer(
                &resource_signer,
                metadata,
                recipient,
                amount_u64
            );
        };

        event::emit(
            FinTransfer {
                origin_chain,
                origin_nonce,
                token_address,
                amount,
                recipient,
                fee_recipient: payload.transfer_fee_recipient(),
                message: payload.transfer_message()
            }
        );
    }

    /// Start an outbound transfer from Aptos to another chain.
    ///
    /// `recipient` and `message` are opaque to this module — they get
    /// emitted verbatim and decoded by the NEAR side.
    public entry fun init_transfer(
        sender: &signer,
        token_address: address,
        amount: u128,
        fee: u128,
        native_fee: u128,
        recipient: String,
        message: vector<u8>
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert!(
            (state.pause_flags & PAUSE_INIT_TRANSFER) == 0,
            E_INIT_TRANSFER_PAUSED
        );
        assert!(amount > 0, E_ZERO_AMOUNT);
        assert!(fee < amount, E_INVALID_FEE);

        // Match the EVM/Starknet semantics: increment first, then use.
        state.current_origin_nonce += 1;
        let origin_nonce = state.current_origin_nonce;

        assert!(amount <= MAX_U64_AS_U128, E_AMOUNT_OVERFLOW);
        let amount_u64 = (amount as u64);

        let sender_addr = sender.address_of();
        let metadata = object::address_to_object<Metadata>(token_address);

        if (bridge_token::is_bridge_token(metadata)) {
            bridge_token::burn(metadata, sender_addr, amount_u64);
        } else {
            let resource_addr = bridge_object_address();
            primary_fungible_store::transfer(sender, metadata, resource_addr, amount_u64);
        };

        if (native_fee > 0) {
            assert!(native_fee <= MAX_U64_AS_U128, E_AMOUNT_OVERFLOW);
            let resource_addr = bridge_object_address();
            primary_fungible_store::transfer(
                sender,
                state.native_token_metadata,
                resource_addr,
                (native_fee as u64)
            );
        };

        event::emit(
            InitTransfer {
                sender: sender_addr,
                token_address,
                origin_nonce,
                amount,
                fee,
                native_fee,
                recipient,
                message
            }
        );
    }

    // -------- Views --------

    #[view]
    public fun is_transfer_finalised(nonce: u64): bool {
        let state = &BridgeState[bridge_object_address()];
        is_nonce_used(&state.completed_transfers, nonce)
    }

    #[view]
    public fun get_token_address(near_token_id: String): Option<address> {
        let state = &BridgeState[bridge_object_address()];
        let token_id_hash = aptos_hash::keccak256(*near_token_id.bytes());
        if (state.near_to_aptos_token.contains(token_id_hash)) {
            option::some(*state.near_to_aptos_token.borrow(token_id_hash))
        } else {
            option::none()
        }
    }

    #[view]
    /// True iff `metadata` was deployed by this bridge. Authoritative —
    /// reads the `BridgeTokenRefs` resource on the FA object directly.
    public fun is_bridge_token(metadata: Object<Metadata>): bool {
        bridge_token::is_bridge_token(metadata)
    }

    #[view]
    public fun current_origin_nonce(): u64 {
        BridgeState[bridge_object_address()].current_origin_nonce
    }

    #[view]
    public fun pause_flags(): u8 {
        BridgeState[bridge_object_address()].pause_flags
    }

    #[view]
    public fun chain_id(): u8 {
        BridgeState[bridge_object_address()].chain_id
    }

    // -------- Internal --------

    fun assert_admin(state: &BridgeState, who: &signer) {
        assert!(who.address_of() == state.admin, E_NOT_ADMIN);
    }

    fun verify_signature(
        state: &BridgeState,
        message: vector<u8>,
        signature_rs: vector<u8>,
        signature_v: u8
    ) {
        utils::verify_eth_signature(
            message,
            signature_rs,
            signature_v,
            state.near_bridge_derived_address
        );
    }

    fun nonce_slot_and_bit(nonce: u64): (u64, u128) {
        let slot = nonce / BITMAP_WIDTH;
        let bit_pos = nonce % BITMAP_WIDTH;
        let bit = 1u128 << (bit_pos as u8);
        (slot, bit)
    }

    fun is_nonce_used(bitmap: &Table<u64, u128>, nonce: u64): bool {
        let (slot, bit) = nonce_slot_and_bit(nonce);
        if (!bitmap.contains(slot)) {
            return false
        };
        let word = *bitmap.borrow(slot);
        (word & bit) != 0
    }

    fun mark_nonce_used(bitmap: &mut Table<u64, u128>, nonce: u64) {
        let (slot, bit) = nonce_slot_and_bit(nonce);
        if (bitmap.contains(slot)) {
            let word = bitmap.borrow_mut(slot);
            *word |= bit;
        } else {
            bitmap.add(slot, bit);
        };
    }

    // -------- Test helpers --------

    #[test_only]
    public fun test_initialize(
        deployer: &signer,
        near_bridge_derived_address: vector<u8>,
        chain_id: u8,
        native_token_metadata: Object<Metadata>
    ) {
        initialize(
            deployer,
            near_bridge_derived_address,
            chain_id,
            native_token_metadata
        );
    }

    #[test_only]
    public fun test_mark_nonce_used(nonce: u64) {
        let state = &mut BridgeState[bridge_object_address()];
        mark_nonce_used(&mut state.completed_transfers, nonce);
    }

    #[test_only]
    public fun test_nonce_slot_and_bit(nonce: u64): (u64, u128) {
        nonce_slot_and_bit(nonce)
    }
}

