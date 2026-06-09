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
    use std::string::{Self, String};
    use std::option::{Self, Option};
    use aptos_std::table::{Self, Table};
    use aptos_framework::aptos_coin::AptosCoin;
    use aptos_framework::coin;
    use aptos_framework::event;
    use aptos_framework::fungible_asset::{Self, Metadata};
    use aptos_framework::object::{Self, ExtendRef, Object};
    use aptos_framework::primary_fungible_store;
    use wormhole::emitter::EmitterCapability;
    use wormhole::state as wormhole_state;
    use wormhole::wormhole;

    use omni_bridge::bridge_token;
    use omni_bridge::bridge_types;
    use omni_bridge::utils;

    // -------- Errors --------
    // Numeric values are part of the test contract — never reorder.
    // Grouped: lifecycle → auth → pause → deploy → transfer → metadata.

    // Lifecycle / auth
    const E_ALREADY_INITIALIZED: u64 = 1;
    /// Caller does not hold the role required for this call. The role
    /// being checked is implicit from the entry point — see the assert
    /// site for which `ROLE_*` was required.
    const E_UNAUTHORIZED: u64 = 2;

    // Pause flags
    const E_INIT_TRANSFER_PAUSED: u64 = 3;
    const E_FIN_TRANSFER_PAUSED: u64 = 4;
    const E_DEPLOY_TOKEN_PAUSED: u64 = 5;

    // Deploy token
    const E_TOKEN_ALREADY_DEPLOYED: u64 = 6;

    // Transfer
    const E_NONCE_ALREADY_USED: u64 = 7;
    const E_ZERO_AMOUNT: u64 = 8;
    const E_INVALID_FEE: u64 = 9;
    const E_AMOUNT_OVERFLOW: u64 = 10;

    // Metadata
    const E_NOT_BRIDGE_TOKEN: u64 = 11;

    // Role management
    /// Revoking would leave the `Admin` role with zero holders — would
    /// brick the bridge (no one could grant/revoke roles again).
    const E_CANNOT_REMOVE_LAST_ADMIN: u64 = 12;

    // Wormhole
    /// `enable_wormhole` was called when the bridge already holds an
    /// `EmitterCapability`. Re-registering would leak the prior cap.
    const E_WORMHOLE_ALREADY_ENABLED: u64 = 13;

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

    /// Privileged on-chain role discriminants. Each role is a `u8` so it
    /// can be passed across the entry/view-function boundary (Aptos
    /// forbids custom enum types as transaction parameters). Adding a new
    /// role only requires:
    ///   1. add a `ROLE_*` const + a `role_*()` public constructor here
    ///   2. populate it in `initialize` (or grant it via `Admin` using
    ///      `set_role`)
    ///   3. assert it at the gated call site
    ///
    /// Numeric values are part of the on-chain ABI — never reorder.
    const ROLE_ADMIN: u8 = 0;
    const ROLE_PAUSER: u8 = 1;
    const ROLE_METADATA_ADMIN: u8 = 2;

    /// Top-level bridge state. Stored as a resource on the bridge object
    /// (a named Object owned by `@omni_bridge`).
    struct BridgeState has key {
        /// Role discriminant → list of holder addresses. Each role can
        /// have any number of holders; all of them are equally privileged.
        /// See the `ROLE_*` constants for the discriminant values.
        roles: Table<u8, vector<address>>,
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
        /// near_token_id → FA metadata object address.
        near_to_aptos_token: Table<String, address>,
        /// ExtendRef for the bridge object. Used to derive the object's signer
        /// on demand for:
        ///   - creating new FA objects in `deploy_token`
        ///   - moving locked tokens out in `fin_transfer` (non-bridge tokens)
        extend_ref: ExtendRef,
        /// Wormhole emitter capability. `None` until `ROLE_ADMIN` calls
        /// `enable_wormhole`. When `Some`, every public bridge action
        /// (`init_transfer`, `fin_transfer`, `deploy_token`, `log_metadata`)
        /// also publishes a Wormhole VAA whose payload mirrors the EVM
        /// `OmniBridgeWormhole.sol` byte layout. The caller of each entry
        /// pays the Wormhole message fee in `Coin<AptosCoin>` from their
        /// own balance.
        wormhole_emitter: Option<EmitterCapability>
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

    // Emitted on `set_token_metadata`. `icon_uri` / `project_uri` are
    // `None` for fields the caller did not change.
    #[event]
    struct TokenMetadataChanged has drop, store {
        token_address: address,
        icon_uri: Option<String>,
        project_uri: Option<String>,
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
        assert!(deployer_addr == @omni_bridge, E_UNAUTHORIZED);
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

        // Seed each role with the deployer as the sole initial holder.
        let roles = table::new<u8, vector<address>>();
        roles.add(ROLE_ADMIN, vector[deployer_addr]);
        roles.add(ROLE_PAUSER, vector[deployer_addr]);
        roles.add(ROLE_METADATA_ADMIN, vector[deployer_addr]);

        move_to(
            &object_signer,
            BridgeState {
                roles,
                pause_flags: 0,
                near_bridge_derived_address,
                chain_id,
                current_origin_nonce: 0,
                completed_transfers: table::new<u64, u128>(),
                native_token_metadata,
                near_to_aptos_token: table::new<String, address>(),
                extend_ref,
                wormhole_emitter: option::none()
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

    /// Add `new_holder` to the set of `role` holders. No-op if the
    /// address already holds the role. Caller must hold `ROLE_ADMIN`.
    /// Works for every role, including `ROLE_ADMIN` itself.
    public entry fun grant_role(
        admin: &signer, role: u8, new_holder: address
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_role(state, ROLE_ADMIN, admin, E_UNAUTHORIZED);
        add_role_holder(state, role, new_holder);
    }

    /// Remove `holder` from the set of `role` holders. No-op if the
    /// address does not hold the role. Caller must hold `ROLE_ADMIN`.
    /// Refuses to remove the last `ROLE_ADMIN` holder, which would brick
    /// the bridge's role management.
    public entry fun revoke_role(
        admin: &signer, role: u8, holder: address
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_role(state, ROLE_ADMIN, admin, E_UNAUTHORIZED);
        remove_role_holder(state, role, holder);
    }

    public entry fun set_near_bridge_derived_address(
        admin: &signer, new_address: vector<u8>
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_role(state, ROLE_ADMIN, admin, E_UNAUTHORIZED);
        state.near_bridge_derived_address = new_address;
    }

    public entry fun set_pause_flags(admin: &signer, flags: u8) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_role(state, ROLE_ADMIN, admin, E_UNAUTHORIZED);
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
        assert_role(state, ROLE_PAUSER, pauser, E_UNAUTHORIZED);
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

    /// Register the bridge as a Wormhole emitter. Once enabled, every
    /// successful `init_transfer`, `fin_transfer`, `deploy_token`, and
    /// `log_metadata` also publishes a Wormhole VAA whose payload mirrors
    /// the corresponding `OmniBridgeWormhole.sol` extension. Callable once;
    /// re-enabling aborts to avoid leaking the prior emitter cap. The
    /// caller of each subsequent bridge action pays the Wormhole message
    /// fee in `Coin<AptosCoin>` from their own balance.
    public entry fun enable_wormhole(admin: &signer) {
        let state = &mut BridgeState[bridge_object_address()];
        assert_role(state, ROLE_ADMIN, admin, E_UNAUTHORIZED);
        assert!(state.wormhole_emitter.is_none(), E_WORMHOLE_ALREADY_ENABLED);
        state.wormhole_emitter.fill(wormhole::register_emitter());
    }

    /// Update mutable metadata (`icon_uri`, `project_uri`) on a
    /// bridge-deployed FA. `None` fields are left unchanged. Gated on the
    /// `metadata_admin` role (separate from the main admin so metadata
    /// refreshes don't require the high-privilege admin key). Aborts if
    /// the address is not a bridge-deployed token.
    public entry fun set_token_metadata(
        metadata_admin: &signer,
        token_address: address,
        icon_uri: Option<String>,
        project_uri: Option<String>
    ) {
        let state = &BridgeState[bridge_object_address()];
        assert_role(
            state,
            ROLE_METADATA_ADMIN,
            metadata_admin,
            E_UNAUTHORIZED
        );

        let metadata = object::address_to_object<Metadata>(token_address);
        assert!(bridge_token::is_bridge_token(metadata), E_NOT_BRIDGE_TOKEN);

        bridge_token::mutate_metadata(metadata, icon_uri, project_uri);

        event::emit(
            TokenMetadataChanged {
                token_address,
                icon_uri,
                project_uri,
                admin: metadata_admin.address_of()
            }
        );
    }

    // -------- Token discovery --------

    /// Permissionless: emit a `LogMetadata` event describing an existing FA.
    /// The NEAR side picks this event up to decide whether to sign a
    /// `deploy_token` payload for the mirror token on its side. `caller`
    /// pays the Wormhole message fee if Wormhole is enabled; the signer is
    /// otherwise not read on-chain.
    public entry fun log_metadata(
        caller: &signer, token: Object<Metadata>
    ) {
        let state = &mut BridgeState[bridge_object_address()];
        let token_address = token.object_address();
        let name = fungible_asset::name(token);
        let symbol = fungible_asset::symbol(token);
        let decimals = fungible_asset::decimals(token);
        event::emit(
            LogMetadata { token_address, name, symbol, decimals }
        );

        if (is_wormhole_enabled(state)) {
            let payload =
                bridge_types::log_metadata_wormhole_payload(
                    state.chain_id,
                    token_address,
                    &name,
                    &symbol,
                    decimals
                );
            publish_wormhole(state, caller, payload);
        };
    }

    // -------- Bridge operations --------

    /// Deploy a bridged FA token. Anyone may submit the transaction —
    /// security comes from the NEAR MPC signature over the payload, not
    /// access control. `caller` pays the Wormhole message fee if Wormhole
    /// is enabled; the signer is otherwise not read on-chain.
    public entry fun deploy_token(
        caller: &signer,
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

        let token_id = payload.metadata_token();
        assert!(
            !state.near_to_aptos_token.contains(token_id),
            E_TOKEN_ALREADY_DEPLOYED
        );

        let normalized_decimals = utils::normalize_decimals(payload.metadata_decimals());

        let resource_signer = state.extend_ref.generate_signer_for_extending();
        let metadata =
            bridge_token::create(
                &resource_signer,
                // Use the NEAR token id's UTF-8 bytes directly as the seed.
                // Same token id → same deterministic FA object address.
                *token_id.bytes(),
                payload.metadata_name(),
                payload.metadata_symbol(),
                normalized_decimals
            );

        let token_addr = metadata.object_address();
        state.near_to_aptos_token.add(token_id, token_addr);
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

        if (is_wormhole_enabled(state)) {
            let wh_payload =
                bridge_types::deploy_token_wormhole_payload(
                    state.chain_id,
                    &payload.metadata_token(),
                    token_addr,
                    normalized_decimals,
                    payload.metadata_decimals()
                );
            publish_wormhole(state, caller, wh_payload);
        };
    }

    /// Finalize an inbound transfer from another chain. Permissionless —
    /// the NEAR MPC signature is the authorization. `caller` pays the
    /// Wormhole message fee if Wormhole is enabled; the signer is
    /// otherwise not read on-chain.
    public entry fun fin_transfer(
        caller: &signer,
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

        if (is_wormhole_enabled(state)) {
            let fr = payload.transfer_fee_recipient();
            let wh_payload =
                bridge_types::fin_transfer_wormhole_payload(
                    state.chain_id,
                    origin_chain,
                    origin_nonce,
                    token_address,
                    amount,
                    &fr
                );
            publish_wormhole(state, caller, wh_payload);
        };
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

        if (is_wormhole_enabled(state)) {
            let wh_payload =
                bridge_types::init_transfer_wormhole_payload(
                    state.chain_id,
                    sender_addr,
                    token_address,
                    origin_nonce,
                    amount,
                    fee,
                    native_fee,
                    &recipient,
                    &message
                );
            publish_wormhole(state, sender, wh_payload);
        };
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
        if (state.near_to_aptos_token.contains(near_token_id)) {
            option::some(*state.near_to_aptos_token.borrow(near_token_id))
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

    // Return all addresses currently holding `role`. Empty vector if the
    // role has never been populated (won't happen for roles seeded in
    // `initialize`).
    #[view]
    public fun role_holders(role: u8): vector<address> {
        read_role(&BridgeState[bridge_object_address()], role)
    }

    // True if `addr` is one of the holders of `role`.
    #[view]
    public fun has_role(role: u8, addr: address): bool {
        is_role_holder(&BridgeState[bridge_object_address()], role, addr)
    }

    // -------- Role registry --------

    /// Human-readable name + numeric id for a role. Returned by
    /// `all_roles()` so off-chain callers can discover the role table
    /// without hardcoding `ROLE_*` constants.
    struct RoleInfo has copy, drop, store {
        name: String,
        id: u8
    }

    public fun role_info_name(self: &RoleInfo): String {
        self.name
    }
    public fun
 role_info_id(self: &RoleInfo): u8 {
        self.id
    }

    #[view]
    public fun all_roles(): vector<RoleInfo> {
        vector[
            RoleInfo { name: string::utf8(b"Admin"), id: ROLE_ADMIN },
            RoleInfo { name: string::utf8(b"Pauser"), id: ROLE_PAUSER },
            RoleInfo { name: string::utf8(b"MetadataAdmin"), id: ROLE_METADATA_ADMIN }
        ]
    }

    // -------- Internal --------

    /// Read the list of addresses currently holding `role`. Returns an
    /// empty vector if the role has never been populated.
    fun read_role(state: &BridgeState, role: u8): vector<address> {
        if (state.roles.contains(role)) {
            *state.roles.borrow(role)
        } else {
            vector[]
        }
    }

    /// True if `addr` is one of the holders of `role`.
    fun is_role_holder(state: &BridgeState, role: u8, addr: address): bool {
        if (!state.roles.contains(role)) {
            return false
        };
        let holders = state.roles.borrow(role);
        let i = 0;
        let len = holders.length();
        while (i < len) {
            if (holders[i] == addr) {
                return true
            };
            i += 1;
        };
        false
    }

    /// Add `addr` to `role`. No-op if already present. Caller MUST have
    /// already authorized the change.
    fun add_role_holder(
        state: &mut BridgeState, role: u8, addr: address
    ) {
        if (!state.roles.contains(role)) {
            state.roles.add(role, vector[addr]);
            return
        };
        let holders = state.roles.borrow_mut(role);
        if (!contains_address(holders, addr)) {
            holders.push_back(addr);
        };
    }

    /// Remove `addr` from `role`. No-op if not present. Refuses to remove
    /// the last `Admin` holder to keep the bridge governable.
    fun remove_role_holder(
        state: &mut BridgeState, role: u8, addr: address
    ) {
        if (!state.roles.contains(role)) { return };
        let holders = state.roles.borrow_mut(role);
        let (found, idx) = find_address(holders, addr);
        if (!found) { return };
        if (role == ROLE_ADMIN) {
            assert!(holders.length() > 1, E_CANNOT_REMOVE_LAST_ADMIN);
        };
        holders.remove(idx);
    }

    fun contains_address(v: &vector<address>, addr: address): bool {
        let i = 0;
        let len = v.length();
        while (i < len) {
            if (v[i] == addr) {
                return true
            };
            i += 1;
        };
        false
    }

    fun find_address(v: &vector<address>, addr: address): (bool, u64) {
        let i = 0;
        let len = v.length();
        while (i < len) {
            if (v[i] == addr) {
                return (true, i)
            };
            i += 1;
        };
        (false, 0)
    }

    /// Assert that `who` holds `role`, aborting with `err` otherwise.
    fun assert_role(
        state: &BridgeState,
        role: u8,
        who: &signer,
        err: u64
    ) {
        assert!(
            is_role_holder(state, role, who.address_of()),
            err
        );
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

    /// True iff `enable_wormhole` has been called. Each entry point uses
    /// this to short-circuit Wormhole payload construction when the
    /// integration is off, avoiding a several-hundred-byte allocation
    /// + memcopy per transfer in the common (Wormhole-disabled) case.
    inline fun is_wormhole_enabled(state: &BridgeState): bool {
        state.wormhole_emitter.is_some()
    }

    /// Publish `payload` as a Wormhole VAA. Caller must have already
    /// confirmed `is_wormhole_enabled(state)` — borrowing the emitter
    /// here when it's `None` would abort. The Wormhole nonce is `0` for
    /// every message; the bridge's own `current_origin_nonce` (carried
    /// inside the payload) is the replay-prevention identifier. `payer`
    /// funds the Wormhole message fee from `Coin<AptosCoin>`. With the
    /// current Wormhole `message_fee` at 0 the withdrawal is a no-op,
    /// but we still wire the path through so a future fee increase
    /// doesn't require a contract upgrade.
    fun publish_wormhole(
        state: &mut BridgeState, payer: &signer, payload: vector<u8>
    ) {
        let fee = wormhole_state::get_message_fee();
        let fee_coin = coin::withdraw<AptosCoin>(payer, fee);
        wormhole::publish_message(
            state.wormhole_emitter.borrow_mut(),
            0u64,
            payload,
            fee_coin
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

