/// Sui side of the NEAR Omni Bridge.
///
/// Cross-chain bridge contract enabling token transfers between Sui and
/// other chains via NEAR Protocol. All transfers route through NEAR
/// (Sui <-> NEAR <-> other chain). Security is rooted in Ethereum-style
/// ECDSA signatures by the NEAR MPC, verified against
/// `near_bridge_derived_address`; Sui -> NEAR proofs are MPC reads of the
/// events emitted here (no Wormhole).
///
/// See [aptos/sources/omni_bridge.move] and
/// [starknet/src/omni_bridge.cairo] for the sibling implementations whose
/// payload encodings this module mirrors. Sui-specific differences:
///   - Coins are types, not addresses: the wire-format `token_address` is
///     `keccak256(canonical type string of T)` (see `utils`).
///   - Coins cannot be created at runtime (one-time-witness rule), so
///     `deploy_token<T>` binds a `TreasuryCap<T>` from a pre-published
///     per-token package instead of creating the token itself.
///   - State lives in one shared `BridgeState` object; `init` cannot take
///     parameters, so the MPC signer address and chain id are set by a
///     one-shot admin `initialize` call after publish.
module omni_bridge::omni_bridge;

use omni_bridge::bridge_types;
use omni_bridge::utils;
use std::string::String;
use std::type_name::{Self, TypeName};
use sui::bag::{Self, Bag};
use sui::balance::Balance;
use sui::coin::{Self, Coin, CoinMetadata, TreasuryCap};
use sui::coin_registry;
use sui::event;
use sui::object_bag::{Self, ObjectBag};
use sui::package::UpgradeCap;
use sui::sui::SUI;
use sui::table::{Self, Table};

// -------- Errors --------
// Values 1-12 match the Aptos contract; 13+ are Sui-specific.
// Numeric values are part of the test contract - never reorder.

// Lifecycle / auth
const E_ALREADY_INITIALIZED: u64 = 1;
/// Caller does not hold the role required for this call. The role
/// being checked is implicit from the entry point - see the assert
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
/// Revoking would leave the `Admin` role with zero holders - would
/// brick the bridge (no one could grant/revoke roles again).
const E_CANNOT_REMOVE_LAST_ADMIN: u64 = 12;

// Sui-specific
/// `initialize` has not been called yet (no MPC signer configured).
const E_NOT_INITIALIZED: u64 = 13;
/// Shared object version does not match this package version - the
/// caller is running against stale (or too-new) code. See `migrate`.
const E_WRONG_VERSION: u64 = 14;
/// The coin type is already registered with the bridge.
const E_TYPE_ALREADY_USED: u64 = 15;
/// `deploy_token` requires a `TreasuryCap` with zero total supply.
const E_SUPPLY_NOT_ZERO: u64 = 16;
/// The `UpgradeCap` does not control the coin's defining package at
/// version 1.
const E_INVALID_UPGRADE_CAP: u64 = 17;
/// `CoinMetadata` does not match the MPC-signed metadata payload.
const E_METADATA_MISMATCH: u64 = 18;
/// `migrate` called but the shared object is already at this version.
const E_NOT_MIGRATION: u64 = 19;
/// The MPC signer address must be exactly 20 bytes.
const E_INVALID_DERIVED_ADDRESS: u64 = 20;
/// `chain_id` must be non-zero (0 is the unconfigured sentinel).
const E_INVALID_CHAIN_ID: u64 = 21;

/// Largest amount that fits in `u64`, used to bound `u128` payload
/// amounts before they're handed to the Sui coin APIs.
const MAX_U64_AS_U128: u128 = 0xFFFFFFFFFFFFFFFF;

// -------- Pause flags --------

const PAUSE_INIT_TRANSFER: u8 = 0x01;
const PAUSE_FIN_TRANSFER: u8 = 0x02;
const PAUSE_DEPLOY_TOKEN: u8 = 0x04;
const PAUSE_ALL: u8 = 0xFF;

/// Bitmap word width - keep at 128 so each entry packs nonces
/// [n*128, n*128+127].
const BITMAP_WIDTH: u64 = 128;

/// Version of the `BridgeState` shared-object layout/logic. Every entry
/// point asserts it so users cannot execute against a stale package after
/// an upgrade (Sui upgrades create a new package address while the old
/// code keeps running). Bump on upgrade and ship a `migrate` path.
const VERSION: u64 = 1;

/// Privileged on-chain role discriminants. Each role is a `u8` so the
/// numbering stays aligned with the Aptos/Starknet siblings. Adding a
/// new role only requires:
///   1. add a `ROLE_*` const + an entry in `all_roles()`
///   2. populate it in `init` (or grant it via `Admin` using `grant_role`)
///   3. assert it at the gated call site
///
/// Numeric values are part of the on-chain ABI - never reorder.
const ROLE_ADMIN: u8 = 0;
const ROLE_PAUSER: u8 = 1;
const ROLE_METADATA_ADMIN: u8 = 2;

/// Top-level bridge state: a single shared object created at publish.
public struct BridgeState has key {
    id: UID,
    /// See `VERSION`.
    version: u64,
    /// Role discriminant -> list of holder addresses. Each role can
    /// have any number of holders; all of them are equally privileged.
    /// See the `ROLE_*` constants for the discriminant values.
    roles: Table<u8, vector<address>>,
    /// Bitfield of paused operations. See `PAUSE_*` constants.
    pause_flags: u8,
    /// 20-byte recovered address of the NEAR MPC-derived Ethereum signer.
    /// Empty until `initialize` - every bridge operation aborts until set.
    near_bridge_derived_address: vector<u8>,
    /// Chain id of this bridge instance (mixed into transfer payload
    /// hashes to prevent cross-chain replay). The `ChainKind::Sui`
    /// discriminant on NEAR.
    chain_id: u8,
    /// Monotonically increasing origin nonce assigned to outbound
    /// transfers.
    current_origin_nonce: u64,
    /// Bitmap of finalized destination nonces. `slot = nonce / 128`,
    /// `bit = nonce % 128`.
    completed_transfers: Table<u64, u128>,
    /// Locked-coin custody (and collected SUI native fees):
    /// `TypeName -> Balance<T>`.
    custody: Bag,
    /// Bridge-deployed token treasuries: `TypeName -> TreasuryCap<T>`.
    /// Presence of a type here is the single source of truth for
    /// "is a bridge token".
    treasuries: ObjectBag,
    /// `CoinMetadata<T>` objects surrendered by `deploy_token`, kept
    /// bridge-owned so `set_token_metadata` can mutate them:
    /// `TypeName -> CoinMetadata<T>`.
    metadata_objects: ObjectBag,
    /// NEAR token account id -> bridge-deployed coin type.
    near_to_sui_token: Table<String, TypeName>,
    /// keccak256(type string) -> coin type. Reverse map so relayers and
    /// indexers can resolve wire-format token ids to concrete types.
    /// Populated by `log_metadata` and `deploy_token`.
    token_registry: Table<address, TypeName>,
}

// -------- Events --------

public struct InitTransfer has copy, drop {
    sender: address,
    token_address: address,
    coin_type: String,
    origin_nonce: u64,
    amount: u128,
    fee: u128,
    native_fee: u128,
    recipient: String,
    message: vector<u8>,
}

public struct FinTransfer has copy, drop {
    origin_chain: u8,
    origin_nonce: u64,
    token_address: address,
    coin_type: String,
    amount: u128,
    recipient: address,
    fee_recipient: Option<String>,
    message: vector<u8>,
}

public struct DeployToken has copy, drop {
    token_address: address,
    coin_type: String,
    near_token_id: String,
    name: String,
    symbol: String,
    decimals: u8,
    origin_decimals: u8,
}

public struct LogMetadata has copy, drop {
    token_address: address,
    coin_type: String,
    name: String,
    symbol: String,
    decimals: u8,
}

public struct PauseStateChanged has copy, drop {
    old_flags: u8,
    new_flags: u8,
    admin: address,
}

// Emitted on `set_token_metadata`. `description` / `icon_url` are
// `None` for fields the caller did not change.
public struct TokenMetadataChanged has copy, drop {
    token_address: address,
    coin_type: String,
    description: Option<String>,
    icon_url: Option<std::ascii::String>,
    admin: address,
}

// -------- Initialization --------

/// Runs exactly once at package publish. Creates the shared bridge state
/// with the publisher as the sole holder of every role. The MPC signer
/// address and chain id cannot be passed here (Sui `init` takes no
/// parameters) - call `initialize` next.
fun init(ctx: &mut TxContext) {
    let sender = ctx.sender();
    let mut roles = table::new<u8, vector<address>>(ctx);
    roles.add(ROLE_ADMIN, vector[sender]);
    roles.add(ROLE_PAUSER, vector[sender]);
    roles.add(ROLE_METADATA_ADMIN, vector[sender]);

    transfer::share_object(BridgeState {
        id: object::new(ctx),
        version: VERSION,
        roles,
        pause_flags: 0,
        near_bridge_derived_address: vector[],
        chain_id: 0,
        current_origin_nonce: 0,
        completed_transfers: table::new(ctx),
        custody: bag::new(ctx),
        treasuries: object_bag::new(ctx),
        metadata_objects: object_bag::new(ctx),
        near_to_sui_token: table::new(ctx),
        token_registry: table::new(ctx),
    });
}

/// One-shot post-publish configuration. Callable once by an `Admin`;
/// every bridge operation aborts with `E_NOT_INITIALIZED` until this
/// has run. `chain_id` is the `ChainKind::Sui` discriminant on NEAR
/// (expected 14) — it is interleaved as the OmniAddress tag byte in every
/// signed transfer payload, so it MUST match NEAR's discriminant or all
/// inbound `fin_transfer`s fail signature verification. `0` is rejected
/// (it is the unconfigured sentinel); a wrong non-zero value is
/// recoverable via `set_chain_id`.
public fun initialize(
    state: &mut BridgeState,
    near_bridge_derived_address: vector<u8>,
    chain_id: u8,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    assert!(state.near_bridge_derived_address.is_empty(), E_ALREADY_INITIALIZED);
    assert!(near_bridge_derived_address.length() == 20, E_INVALID_DERIVED_ADDRESS);
    assert!(chain_id != 0, E_INVALID_CHAIN_ID);
    state.near_bridge_derived_address = near_bridge_derived_address;
    state.chain_id = chain_id;
}

// -------- Admin --------

/// Add `new_holder` to the set of `role` holders. No-op if the
/// address already holds the role. Caller must hold `ROLE_ADMIN`.
/// Works for every role, including `ROLE_ADMIN` itself.
public fun grant_role(
    state: &mut BridgeState,
    role: u8,
    new_holder: address,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    add_role_holder(state, role, new_holder);
}

/// Remove `holder` from the set of `role` holders. No-op if the
/// address does not hold the role. Caller must hold `ROLE_ADMIN`.
/// Refuses to remove the last `ROLE_ADMIN` holder, which would brick
/// the bridge's role management.
public fun revoke_role(
    state: &mut BridgeState,
    role: u8,
    holder: address,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    remove_role_holder(state, role, holder);
}

/// Rotate the NEAR MPC signer address. Admin-only.
public fun set_near_bridge_derived_address(
    state: &mut BridgeState,
    new_address: vector<u8>,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    assert_configured(state);
    assert!(new_address.length() == 20, E_INVALID_DERIVED_ADDRESS);
    state.near_bridge_derived_address = new_address;
}

/// Correct the configured `chain_id`. Admin-only. Needed because
/// `chain_id` is baked into the signed transfer-payload preimage: a wrong
/// value silently rejects every inbound transfer, and `initialize` is
/// one-shot, so without this setter a misconfiguration would be an
/// unrecoverable brick. Rejects `0` (the unconfigured sentinel).
public fun set_chain_id(state: &mut BridgeState, new_chain_id: u8, ctx: &TxContext) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    assert_configured(state);
    assert!(new_chain_id != 0, E_INVALID_CHAIN_ID);
    state.chain_id = new_chain_id;
}

public fun set_pause_flags(state: &mut BridgeState, flags: u8, ctx: &TxContext) {
    assert_version(state);
    assert_role(state, ROLE_ADMIN, ctx.sender());
    let old = state.pause_flags;
    state.pause_flags = flags;
    event::emit(PauseStateChanged {
        old_flags: old,
        new_flags: flags,
        admin: ctx.sender(),
    });
}

public fun pause_all(state: &mut BridgeState, ctx: &TxContext) {
    assert_version(state);
    assert_role(state, ROLE_PAUSER, ctx.sender());
    let old = state.pause_flags;
    state.pause_flags = PAUSE_ALL;
    event::emit(PauseStateChanged {
        old_flags: old,
        new_flags: PAUSE_ALL,
        admin: ctx.sender(),
    });
}

/// Bring the shared object up to this package's `VERSION` after an
/// upgrade. Admin-only. Any state added in future versions must be
/// created here (`init` does not rerun on upgrades).
public fun migrate(state: &mut BridgeState, ctx: &TxContext) {
    assert_role(state, ROLE_ADMIN, ctx.sender());
    assert!(state.version < VERSION, E_NOT_MIGRATION);
    state.version = VERSION;
}

// -------- Token discovery --------

/// Permissionless: emit a `LogMetadata` event describing an existing coin.
/// The NEAR side picks this event up (via MPC read) to decide whether to
/// sign a `deploy_token` payload for the mirror token on its side. Also
/// registers the coin type in `token_registry` so off-chain actors can
/// resolve the 32-byte token id back to the concrete type.
public fun log_metadata<T>(
    state: &mut BridgeState,
    coin_metadata: &CoinMetadata<T>,
) {
    assert_version(state);
    assert_configured(state);

    register_coin_type<T>(state);

    event::emit(LogMetadata {
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        name: coin::get_name(coin_metadata),
        symbol: coin::get_symbol(coin_metadata).to_string(),
        decimals: coin::get_decimals(coin_metadata),
    });
}

/// `log_metadata` variant for coins created under the newer
/// `sui::coin_registry` Currency standard, which may have no legacy
/// `CoinMetadata<T>` object at all.
public fun log_metadata_registry<T>(
    state: &mut BridgeState,
    currency: &coin_registry::Currency<T>,
) {
    assert_version(state);
    assert_configured(state);

    register_coin_type<T>(state);

    event::emit(LogMetadata {
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        name: coin_registry::name(currency),
        symbol: coin_registry::symbol(currency),
        decimals: coin_registry::decimals(currency),
    });
}

// -------- Bridge operations --------

/// Register a bridged coin for a NEAR token. Anyone may submit the
/// transaction - security comes from the NEAR MPC signature over the
/// payload, not access control (parity with the sibling chains).
///
/// Sui cannot create a currency at runtime (`create_currency` needs the
/// one-time witness of `T`), so unlike Aptos the coin arrives
/// pre-published (see `token_template/`) and this call BINDS it to the
/// signed payload. The signed payload cannot name `T`, so the binding is
/// constrained instead:
///   - `treasury_cap` must have zero supply (protects NEAR's
///     locked-token accounting),
///   - `upgrade_cap` must control `T`'s defining package at version 1
///     and is made immutable here (no future upgrades, one coin per
///     package),
///   - `coin_metadata` must match the signed name/symbol and the clamped
///     decimals, and is surrendered to the bridge so only
///     `set_token_metadata` can mutate it.
///
/// Residual risk (accepted, documented in the README): a front-runner
/// can bind their own coin matching all of the above; such a coin is
/// functionally identical unless it was created as a regulated currency,
/// whose retained `DenyCapV2` allows freezing transfers later -
/// regulated-ness is not verifiable on-chain today.
public fun deploy_token<T>(
    state: &mut BridgeState,
    signature: vector<u8>,
    token: String,
    name: String,
    symbol: String,
    decimals: u8,
    treasury_cap: TreasuryCap<T>,
    upgrade_cap: UpgradeCap,
    coin_metadata: CoinMetadata<T>,
) {
    assert_version(state);
    assert_configured(state);
    assert!((state.pause_flags & PAUSE_DEPLOY_TOKEN) == 0, E_DEPLOY_TOKEN_PAUSED);

    let payload = bridge_types::new_metadata_payload(token, name, symbol, decimals);
    let encoded = payload.metadata_to_borsh();
    utils::verify_eth_signature(&encoded, &signature, &state.near_bridge_derived_address);

    let token_id = payload.metadata_token();
    assert!(!state.near_to_sui_token.contains(token_id), E_TOKEN_ALREADY_DEPLOYED);
    let key = type_name::with_defining_ids<T>();
    assert!(!state.treasuries.contains(key), E_TYPE_ALREADY_USED);

    assert!(coin::total_supply(&treasury_cap) == 0, E_SUPPLY_NOT_ZERO);

    assert!(
        upgrade_cap.upgrade_package().to_address() == utils::type_package_address<T>() &&
        upgrade_cap.version() == 1,
        E_INVALID_UPGRADE_CAP,
    );
    sui::package::make_immutable(upgrade_cap);

    let normalized_decimals = utils::normalize_decimals(payload.metadata_decimals());
    assert!(
        coin::get_decimals(&coin_metadata) == normalized_decimals &&
        coin::get_name(&coin_metadata) == payload.metadata_name() &&
        *coin::get_symbol(&coin_metadata).as_bytes() == *payload.metadata_symbol().as_bytes(),
        E_METADATA_MISMATCH,
    );

    state.treasuries.add(key, treasury_cap);
    state.metadata_objects.add(key, coin_metadata);
    state.near_to_sui_token.add(token_id, key);
    register_coin_type<T>(state);

    event::emit(DeployToken {
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        near_token_id: token_id,
        name: payload.metadata_name(),
        symbol: payload.metadata_symbol(),
        decimals: normalized_decimals,
        origin_decimals: payload.metadata_decimals(),
    });
}

/// Update mutable metadata (`description`, `icon_url`) on a
/// bridge-deployed coin. `None` fields are left unchanged. Gated on the
/// `MetadataAdmin` role (separate from the main admin so metadata
/// refreshes don't require the high-privilege admin key). Aborts if `T`
/// is not a bridge-deployed token.
public fun set_token_metadata<T>(
    state: &mut BridgeState,
    description: Option<String>,
    icon_url: Option<std::ascii::String>,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_role(state, ROLE_METADATA_ADMIN, ctx.sender());
    assert!(is_bridge_token<T>(state), E_NOT_BRIDGE_TOKEN);

    let key = type_name::with_defining_ids<T>();
    let cap = state.treasuries.borrow<TypeName, TreasuryCap<T>>(key);
    let coin_metadata =
        state.metadata_objects.borrow_mut<TypeName, CoinMetadata<T>>(key);

    if (description.is_some()) {
        coin::update_description(cap, coin_metadata, *description.borrow());
    };
    if (icon_url.is_some()) {
        coin::update_icon_url(cap, coin_metadata, *icon_url.borrow());
    };

    event::emit(TokenMetadataChanged {
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        description,
        icon_url,
        admin: ctx.sender(),
    });
}

/// Start an outbound transfer from Sui to another chain.
///
/// The full value of `coin` is the transfer amount (callers split an
/// exact coin in the same PTB); `fee` is the token-denominated part of
/// that amount claimable by the relayer on NEAR; `native_fee_coin`'s
/// value is the SUI-denominated relayer fee (pass a zero coin for none).
/// `recipient` and `message` are opaque to this module - they get
/// emitted verbatim and decoded by the NEAR side.
public fun init_transfer<T>(
    state: &mut BridgeState,
    coin: Coin<T>,
    fee: u64,
    native_fee_coin: Coin<SUI>,
    recipient: String,
    message: vector<u8>,
    ctx: &TxContext,
) {
    assert_version(state);
    assert_configured(state);
    assert!((state.pause_flags & PAUSE_INIT_TRANSFER) == 0, E_INIT_TRANSFER_PAUSED);

    let amount = coin.value();
    assert!(amount > 0, E_ZERO_AMOUNT);
    assert!(fee < amount, E_INVALID_FEE);

    // Match the EVM/Starknet/Aptos semantics: increment first, then use.
    state.current_origin_nonce = state.current_origin_nonce + 1;
    let origin_nonce = state.current_origin_nonce;

    if (is_bridge_token<T>(state)) {
        burn_bridge_token(state, coin);
    } else {
        deposit(state, coin.into_balance());
    };

    let native_fee = native_fee_coin.value();
    if (native_fee > 0) {
        // Native fees join the SUI custody balance: they back the wrapped
        // native token minted to the fee recipient on NEAR and can leave
        // custody again through a regular `fin_transfer<SUI>`.
        deposit(state, native_fee_coin.into_balance());
    } else {
        native_fee_coin.destroy_zero();
    };

    event::emit(InitTransfer {
        sender: ctx.sender(),
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        origin_nonce,
        amount: amount as u128,
        fee: fee as u128,
        native_fee: native_fee as u128,
        recipient,
        message,
    });
}

/// Finalize an inbound transfer from another chain. Permissionless -
/// the NEAR MPC signature is the authorization. The transaction sender
/// is not read on-chain.
///
/// The wire-format token id is derived from `T` itself
/// (`keccak256(canonical type string)`), so the signature check binds the
/// generic type argument: submitting with the wrong `T` reconstructs a
/// different payload and the signature verification fails.
public fun fin_transfer<T>(
    state: &mut BridgeState,
    signature: vector<u8>,
    destination_nonce: u64,
    origin_chain: u8,
    origin_nonce: u64,
    amount: u128,
    recipient: address,
    fee_recipient: Option<String>,
    message: vector<u8>,
    ctx: &mut TxContext,
) {
    assert_version(state);
    assert_configured(state);
    assert!((state.pause_flags & PAUSE_FIN_TRANSFER) == 0, E_FIN_TRANSFER_PAUSED);

    // Replay protection before anything else (checks-effects-interactions).
    assert!(
        !is_nonce_used(&state.completed_transfers, destination_nonce),
        E_NONCE_ALREADY_USED,
    );
    mark_nonce_used(&mut state.completed_transfers, destination_nonce);

    let payload = bridge_types::new_transfer_message_payload(
        destination_nonce,
        origin_chain,
        origin_nonce,
        utils::token_address<T>(),
        amount,
        recipient,
        fee_recipient,
        message,
    );
    let encoded = payload.transfer_message_to_borsh(state.chain_id);
    utils::verify_eth_signature(&encoded, &signature, &state.near_bridge_derived_address);

    // Cap to u64 - Sui coin amounts are u64.
    assert!(amount <= MAX_U64_AS_U128, E_AMOUNT_OVERFLOW);
    let amount_u64 = amount as u64;

    let coin = if (is_bridge_token<T>(state)) {
        mint_bridge_token<T>(state, amount_u64, ctx)
    } else {
        // Locked-token path: release from bridge custody.
        coin::from_balance(withdraw<T>(state, amount_u64), ctx)
    };
    transfer::public_transfer(coin, recipient);

    event::emit(FinTransfer {
        origin_chain,
        origin_nonce,
        token_address: utils::token_address<T>(),
        coin_type: utils::coin_type_string<T>(),
        amount,
        recipient,
        fee_recipient: payload.transfer_fee_recipient(),
        message: payload.transfer_message(),
    });
}

// -------- Views --------

public fun is_configured(state: &BridgeState): bool {
    !state.near_bridge_derived_address.is_empty()
}

public fun current_origin_nonce(state: &BridgeState): u64 {
    state.current_origin_nonce
}

public fun pause_flags(state: &BridgeState): u8 {
    state.pause_flags
}

public fun chain_id(state: &BridgeState): u8 {
    state.chain_id
}

/// Return all addresses currently holding `role`. Empty vector if the
/// role has never been populated (won't happen for roles seeded in
/// `init`).
public fun role_holders(state: &BridgeState, role: u8): vector<address> {
    if (state.roles.contains(role)) {
        *state.roles.borrow(role)
    } else {
        vector[]
    }
}

/// True if `addr` is one of the holders of `role`.
public fun has_role(state: &BridgeState, role: u8, addr: address): bool {
    is_role_holder(state, role, addr)
}

public fun is_transfer_finalised(state: &BridgeState, nonce: u64): bool {
    is_nonce_used(&state.completed_transfers, nonce)
}

/// True iff `T` was registered by this bridge's `deploy_token` (i.e. the
/// bridge holds its `TreasuryCap`). Authoritative - one source of truth.
public fun is_bridge_token<T>(state: &BridgeState): bool {
    state.treasuries.contains(type_name::with_defining_ids<T>())
}

/// NEAR token id -> deployed coin type string (canonical `type_name`
/// form), if `deploy_token` has run for it.
public fun get_token_address(state: &BridgeState, near_token_id: String): Option<String> {
    if (state.near_to_sui_token.contains(near_token_id)) {
        let coin_type = *state.near_to_sui_token.borrow(near_token_id);
        option::some(coin_type.into_string().to_string())
    } else {
        option::none()
    }
}

/// Resolve a 32-byte wire-format token id back to the canonical coin type
/// string. Populated by `log_metadata` and `deploy_token`.
public fun get_coin_type(state: &BridgeState, token_address: address): Option<String> {
    if (state.token_registry.contains(token_address)) {
        let coin_type = *state.token_registry.borrow(token_address);
        option::some(coin_type.into_string().to_string())
    } else {
        option::none()
    }
}

/// Coin currently held in custody for `T` (locked transfers + collected
/// native fees for `T = SUI`).
public fun locked_balance<T>(state: &BridgeState): u64 {
    let key = type_name::with_defining_ids<T>();
    if (state.custody.contains(key)) {
        state.custody.borrow<TypeName, Balance<T>>(key).value()
    } else {
        0
    }
}

// -------- Role registry --------

/// Human-readable name + numeric id for a role. Returned by
/// `all_roles()` so off-chain callers can discover the role table
/// without hardcoding `ROLE_*` constants.
public struct RoleInfo has copy, drop {
    name: String,
    id: u8,
}

public fun role_info_name(self: &RoleInfo): String {
    self.name
}

public fun role_info_id(self: &RoleInfo): u8 {
    self.id
}

public fun all_roles(): vector<RoleInfo> {
    vector[
        RoleInfo { name: b"Admin".to_string(), id: ROLE_ADMIN },
        RoleInfo { name: b"Pauser".to_string(), id: ROLE_PAUSER },
        RoleInfo { name: b"MetadataAdmin".to_string(), id: ROLE_METADATA_ADMIN },
    ]
}

// -------- Internal: gates --------

fun assert_version(state: &BridgeState) {
    assert!(state.version == VERSION, E_WRONG_VERSION);
}

fun assert_configured(state: &BridgeState) {
    assert!(is_configured(state), E_NOT_INITIALIZED);
}

/// Assert that `who` holds `role`, aborting with `E_UNAUTHORIZED`
/// otherwise.
fun assert_role(state: &BridgeState, role: u8, who: address) {
    assert!(is_role_holder(state, role, who), E_UNAUTHORIZED);
}

// -------- Internal: nonce bitmap --------

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
    (*bitmap.borrow(slot) & bit) != 0
}

fun mark_nonce_used(bitmap: &mut Table<u64, u128>, nonce: u64) {
    let (slot, bit) = nonce_slot_and_bit(nonce);
    if (bitmap.contains(slot)) {
        let word = bitmap.borrow_mut(slot);
        *word = *word | bit;
    } else {
        bitmap.add(slot, bit);
    };
}

// -------- Internal: custody & bridge tokens --------

/// Join `balance` into the custody bag under `T`'s type key.
fun deposit<T>(state: &mut BridgeState, balance: Balance<T>) {
    let key = type_name::with_defining_ids<T>();
    if (state.custody.contains(key)) {
        state.custody.borrow_mut<TypeName, Balance<T>>(key).join(balance);
    } else {
        state.custody.add(key, balance);
    };
}

/// Split `amount` out of the custody bag. Aborts (in `balance::split`) if
/// custody holds less than `amount`; aborts in `bag::borrow_mut` if `T`
/// was never locked at all.
fun withdraw<T>(state: &mut BridgeState, amount: u64): Balance<T> {
    let key = type_name::with_defining_ids<T>();
    state.custody.borrow_mut<TypeName, Balance<T>>(key).split(amount)
}

/// Burn a bridge-deployed coin via its stored `TreasuryCap`.
fun burn_bridge_token<T>(state: &mut BridgeState, coin: Coin<T>) {
    let key = type_name::with_defining_ids<T>();
    coin::burn(state.treasuries.borrow_mut<TypeName, TreasuryCap<T>>(key), coin);
}

/// Mint a bridge-deployed coin via its stored `TreasuryCap`.
fun mint_bridge_token<T>(
    state: &mut BridgeState,
    amount: u64,
    ctx: &mut TxContext,
): Coin<T> {
    let key = type_name::with_defining_ids<T>();
    coin::mint(state.treasuries.borrow_mut<TypeName, TreasuryCap<T>>(key), amount, ctx)
}

/// Record `T` in the wire-id -> type reverse map (idempotent).
fun register_coin_type<T>(state: &mut BridgeState) {
    let token_address = utils::token_address<T>();
    if (!state.token_registry.contains(token_address)) {
        state.token_registry.add(token_address, type_name::with_defining_ids<T>());
    };
}

// -------- Internal: roles --------

/// True if `addr` is one of the holders of `role`.
fun is_role_holder(state: &BridgeState, role: u8, addr: address): bool {
    if (!state.roles.contains(role)) {
        return false
    };
    state.roles.borrow(role).contains(&addr)
}

/// Add `addr` to `role`. No-op if already present. Caller MUST have
/// already authorized the change.
fun add_role_holder(state: &mut BridgeState, role: u8, addr: address) {
    if (!state.roles.contains(role)) {
        state.roles.add(role, vector[addr]);
        return
    };
    let holders = state.roles.borrow_mut(role);
    if (!holders.contains(&addr)) {
        holders.push_back(addr);
    };
}

/// Remove `addr` from `role`. No-op if not present. Refuses to remove
/// the last `Admin` holder to keep the bridge governable.
fun remove_role_holder(state: &mut BridgeState, role: u8, addr: address) {
    if (!state.roles.contains(role)) {
        return
    };
    let holders = state.roles.borrow_mut(role);
    let (found, idx) = holders.index_of(&addr);
    if (!found) {
        return
    };
    if (role == ROLE_ADMIN) {
        assert!(holders.length() > 1, E_CANNOT_REMOVE_LAST_ADMIN);
    };
    holders.remove(idx);
}

// -------- Test helpers --------

#[test_only]
public fun init_for_testing(ctx: &mut TxContext) {
    init(ctx);
}

#[test_only]
public fun test_mark_nonce_used(state: &mut BridgeState, nonce: u64) {
    mark_nonce_used(&mut state.completed_transfers, nonce);
}

#[test_only]
public fun set_version_for_testing(state: &mut BridgeState, version: u64) {
    state.version = version;
}

#[test_only]
public fun test_token_description<T>(state: &BridgeState): String {
    let key = type_name::with_defining_ids<T>();
    coin::get_description(state.metadata_objects.borrow<TypeName, CoinMetadata<T>>(key))
}

#[test_only]
public fun test_token_icon_url<T>(state: &BridgeState): Option<sui::url::Url> {
    let key = type_name::with_defining_ids<T>();
    coin::get_icon_url(state.metadata_objects.borrow<TypeName, CoinMetadata<T>>(key))
}

// Event structs have private fields outside this module, so tests build
// expected values through these constructors and compare whole structs.

#[test_only]
public fun new_init_transfer_event(
    sender: address,
    token_address: address,
    coin_type: String,
    origin_nonce: u64,
    amount: u128,
    fee: u128,
    native_fee: u128,
    recipient: String,
    message: vector<u8>,
): InitTransfer {
    InitTransfer {
        sender,
        token_address,
        coin_type,
        origin_nonce,
        amount,
        fee,
        native_fee,
        recipient,
        message,
    }
}

#[test_only]
public fun new_fin_transfer_event(
    origin_chain: u8,
    origin_nonce: u64,
    token_address: address,
    coin_type: String,
    amount: u128,
    recipient: address,
    fee_recipient: Option<String>,
    message: vector<u8>,
): FinTransfer {
    FinTransfer {
        origin_chain,
        origin_nonce,
        token_address,
        coin_type,
        amount,
        recipient,
        fee_recipient,
        message,
    }
}

#[test_only]
public fun new_deploy_token_event(
    token_address: address,
    coin_type: String,
    near_token_id: String,
    name: String,
    symbol: String,
    decimals: u8,
    origin_decimals: u8,
): DeployToken {
    DeployToken {
        token_address,
        coin_type,
        near_token_id,
        name,
        symbol,
        decimals,
        origin_decimals,
    }
}

#[test_only]
public fun new_log_metadata_event(
    token_address: address,
    coin_type: String,
    name: String,
    symbol: String,
    decimals: u8,
): LogMetadata {
    LogMetadata { token_address, coin_type, name, symbol, decimals }
}
