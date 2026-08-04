#[test_only]
module omni_bridge::omni_bridge_tests;

use omni_bridge::omni_bridge::{Self, BridgeState};
use omni_bridge::test_coin::{Self, TEST_COIN};
use omni_bridge::utils;
use std::string;
use sui::coin;
use sui::event;
use sui::sui::SUI;
use sui::test_scenario::{Self, Scenario};

const ADMIN: address = @0xAD;
const USER: address = @0xB0B;
const CHAIN_ID: u8 = 14;

const ROLE_ADMIN: u8 = 0;
const ROLE_PAUSER: u8 = 1;
const ROLE_METADATA_ADMIN: u8 = 2;

fun derived_address(): vector<u8> {
    vector[
        0xB9, 0x60, 0xBE, 0xD5, 0x3C, 0x17, 0xF9, 0xA0, 0x21, 0x53, 0x8B, 0x5D,
        0x6F, 0x08, 0xE7, 0x46, 0x6B, 0x96, 0x6C, 0x53,
    ]
}

/// Publish-equivalent: run `init` as ADMIN and advance one tx so the
/// shared state can be taken.
fun setup(): Scenario {
    let mut ts = test_scenario::begin(ADMIN);
    omni_bridge::init_for_testing(ts.ctx());
    ts.next_tx(ADMIN);
    ts
}

/// Setup + `initialize` with the test MPC signer and chain id 14.
fun setup_configured(): Scenario {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::initialize(&mut state, derived_address(), CHAIN_ID, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(ADMIN);
    ts
}

// -------- init / initialize --------

#[test]
fun init_seeds_all_roles_to_publisher() {
    let ts = setup();
    let state = ts.take_shared<BridgeState>();
    assert!(state.has_role(ROLE_ADMIN, ADMIN));
    assert!(state.has_role(ROLE_PAUSER, ADMIN));
    assert!(state.has_role(ROLE_METADATA_ADMIN, ADMIN));
    assert!(!state.has_role(ROLE_ADMIN, USER));
    assert!(!state.is_configured());
    assert!(state.pause_flags() == 0);
    assert!(state.current_origin_nonce() == 0);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun initialize_sets_config() {
    let ts = setup_configured();
    let state = ts.take_shared<BridgeState>();
    assert!(state.is_configured());
    assert!(state.chain_id() == CHAIN_ID);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_ALREADY_INITIALIZED)]
fun initialize_twice_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::initialize(&mut state, derived_address(), CHAIN_ID, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun initialize_by_non_admin_aborts() {
    let mut ts = setup();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::initialize(&mut state, derived_address(), CHAIN_ID, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_DERIVED_ADDRESS)]
fun initialize_with_short_address_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    let mut addr = derived_address();
    addr.pop_back();
    omni_bridge::initialize(&mut state, addr, CHAIN_ID, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_CHAIN_ID)]
fun initialize_with_zero_chain_id_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::initialize(&mut state, derived_address(), 0, ts.ctx());
    abort 0
}

#[test]
fun admin_corrects_chain_id() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_chain_id(&mut state, 15, ts.ctx());
    assert!(state.chain_id() == 15);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_CHAIN_ID)]
fun set_chain_id_zero_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_chain_id(&mut state, 0, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun set_chain_id_by_non_admin_aborts() {
    let mut ts = setup_configured();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_chain_id(&mut state, 15, ts.ctx());
    abort 0
}

// -------- roles --------

#[test]
fun grant_role_adds_holder_idempotently() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::grant_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    assert!(state.has_role(ROLE_PAUSER, USER));
    omni_bridge::grant_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    assert!(state.role_holders(ROLE_PAUSER).length() == 2);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun revoke_role_removes_holder() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::grant_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    omni_bridge::revoke_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    assert!(!state.has_role(ROLE_PAUSER, USER));
    // Revoking a non-holder is a no-op.
    omni_bridge::revoke_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun admin_can_step_down_when_second_admin_exists() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::grant_role(&mut state, ROLE_ADMIN, USER, ts.ctx());
    omni_bridge::revoke_role(&mut state, ROLE_ADMIN, ADMIN, ts.ctx());
    assert!(!state.has_role(ROLE_ADMIN, ADMIN));
    assert!(state.has_role(ROLE_ADMIN, USER));
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_CANNOT_REMOVE_LAST_ADMIN)]
fun revoking_last_admin_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::revoke_role(&mut state, ROLE_ADMIN, ADMIN, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun grant_role_by_non_admin_aborts() {
    let mut ts = setup();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::grant_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    abort 0
}

#[test]
fun all_roles_lists_three() {
    assert!(omni_bridge::all_roles().length() == 3);
}

#[test]
fun role_holders_of_unknown_role_is_empty() {
    let ts = setup();
    let state = ts.take_shared<BridgeState>();
    assert!(state.role_holders(99).is_empty());
    test_scenario::return_shared(state);
    ts.end();
}

// -------- pause --------

#[test]
fun admin_sets_pause_flags() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_pause_flags(&mut state, 0x03, ts.ctx());
    assert!(state.pause_flags() == 0x03);
    omni_bridge::set_pause_flags(&mut state, 0x00, ts.ctx());
    assert!(state.pause_flags() == 0x00);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun pauser_can_pause_all() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::grant_role(&mut state, ROLE_PAUSER, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::pause_all(&mut state, ts.ctx());
    assert!(state.pause_flags() == 0xFF);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun pause_all_by_non_pauser_aborts() {
    let mut ts = setup();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::pause_all(&mut state, ts.ctx());
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun set_pause_flags_by_non_admin_aborts() {
    let mut ts = setup();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_pause_flags(&mut state, 0xFF, ts.ctx());
    abort 0
}

// -------- migrate / rotation --------

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_MIGRATION)]
fun migrate_at_current_version_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::migrate(&mut state, ts.ctx());
    abort 0
}

#[test]
fun admin_rotates_derived_address() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let mut rotated = derived_address();
    *(&mut rotated[0]) = 0x00;
    omni_bridge::set_near_bridge_derived_address(&mut state, rotated, ts.ctx());
    test_scenario::return_shared(state);
    ts.end();
}

// -------- version gate --------

#[test]
#[expected_failure(abort_code = omni_bridge::E_WRONG_VERSION)]
fun stale_version_aborts_entry_points() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_version_for_testing(&mut state, 0);
    omni_bridge::init_transfer(
        &mut state,
        coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
fun migrate_from_older_version_succeeds() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_version_for_testing(&mut state, 0);
    omni_bridge::migrate(&mut state, ts.ctx());
    // Entry points work again after migration.
    omni_bridge::init_transfer(
        &mut state,
        coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    assert!(state.current_origin_nonce() == 1);
    test_scenario::return_shared(state);
    ts.end();
}

// -------- init_transfer --------

#[test]
fun init_transfer_locks_coin_and_emits_event() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let coin = coin::mint_for_testing<TEST_COIN>(1_000, ts.ctx());
    omni_bridge::init_transfer(
        &mut state,
        coin,
        10,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[0xAB],
        ts.ctx(),
    );

    assert!(state.locked_balance<TEST_COIN>() == 1_000);
    assert!(state.current_origin_nonce() == 1);

    let events = event::events_by_type<omni_bridge::InitTransfer>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_init_transfer_event(
        ADMIN,
        utils::token_address<TEST_COIN>(),
        utils::coin_type_string<TEST_COIN>(),
        1,
        1_000,
        10,
        0,
        string::utf8(b"near:bob.near"),
        vector[0xAB],
    );
    assert!(events[0] == expected);

    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun init_transfer_collects_native_fee() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let coin = coin::mint_for_testing<TEST_COIN>(500, ts.ctx());
    let native_fee = coin::mint_for_testing<SUI>(50, ts.ctx());
    omni_bridge::init_transfer(
        &mut state,
        coin,
        0,
        native_fee,
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );

    assert!(state.locked_balance<TEST_COIN>() == 500);
    assert!(state.locked_balance<SUI>() == 50);

    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun init_transfer_increments_origin_nonce() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let mut i = 0u64;
    while (i < 3) {
        omni_bridge::init_transfer(
            &mut state,
            coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
            0,
            coin::zero<SUI>(ts.ctx()),
            string::utf8(b"near:bob.near"),
            vector[],
            ts.ctx(),
        );
        i = i + 1;
    };
    assert!(state.current_origin_nonce() == 3);
    assert!(state.locked_balance<TEST_COIN>() == 300);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_ZERO_AMOUNT)]
fun init_transfer_zero_amount_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::init_transfer(
        &mut state,
        coin::zero<TEST_COIN>(ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_FEE)]
fun init_transfer_fee_not_less_than_amount_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::init_transfer(
        &mut state,
        coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
        100,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INIT_TRANSFER_PAUSED)]
fun init_transfer_paused_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_pause_flags(&mut state, 0x01, ts.ctx());
    omni_bridge::init_transfer(
        &mut state,
        coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_INITIALIZED)]
fun init_transfer_unconfigured_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::init_transfer(
        &mut state,
        coin::mint_for_testing<TEST_COIN>(100, ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    abort 0
}

// -------- fin_transfer --------

// Signature over the borsh TransferMessagePayload for:
// dest_nonce=5, origin_chain=1 (Near), origin_nonce=99,
// token=TEST_COIN (keccak of its type string), amount=250,
// recipient=@0xB0B, fee_recipient=Some("relayer.near"), empty message,
// chain_id=14 — signed by the key behind `derived_address()`.
fun fin_signature(): vector<u8> {
    vector[
        0x97, 0x1A, 0xC1, 0x79, 0x65, 0xB9, 0x3A, 0x62, 0xE0, 0x7F, 0x56, 0x11,
        0xA5, 0x46, 0xD9, 0x78, 0xBE, 0x47, 0x3C, 0x54, 0x7C, 0x9C, 0x23, 0x1F,
        0x5F, 0x20, 0x3C, 0x5A, 0x9A, 0x6D, 0xB7, 0xD8, 0x56, 0xE7, 0x54, 0xE0,
        0x51, 0x09, 0xB7, 0x35, 0xBC, 0x95, 0x1D, 0xD9, 0x7F, 0x69, 0x18, 0x8B,
        0x57, 0xDE, 0x99, 0x89, 0xAB, 0xFD, 0xB5, 0x70, 0x32, 0xFB, 0xE9, 0xDA,
        0x70, 0xF2, 0xA7, 0xDE, 0x1C,
    ]
}

fun call_fin_transfer(state: &mut BridgeState, ts: &mut Scenario) {
    omni_bridge::fin_transfer<TEST_COIN>(
        state,
        fin_signature(),
        5, // destination_nonce
        1, // origin_chain
        99, // origin_nonce
        250,
        USER,
        option::some(string::utf8(b"relayer.near")),
        vector[],
        ts.ctx(),
    );
}

fun lock_some_test_coin(state: &mut BridgeState, amount: u64, ts: &mut Scenario) {
    omni_bridge::init_transfer(
        state,
        coin::mint_for_testing<TEST_COIN>(amount, ts.ctx()),
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
}

#[test]
fun fin_transfer_unlocks_to_recipient() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);

    call_fin_transfer(&mut state, &mut ts);

    assert!(state.locked_balance<TEST_COIN>() == 750);
    assert!(state.is_transfer_finalised(5));
    assert!(!state.is_transfer_finalised(4));

    let events = event::events_by_type<omni_bridge::FinTransfer>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_fin_transfer_event(
        1,
        99,
        utils::token_address<TEST_COIN>(),
        utils::coin_type_string<TEST_COIN>(),
        250,
        USER,
        option::some(string::utf8(b"relayer.near")),
        vector[],
    );
    assert!(events[0] == expected);

    test_scenario::return_shared(state);
    // The recipient received the released coin.
    ts.next_tx(USER);
    let received = ts.take_from_address<coin::Coin<TEST_COIN>>(USER);
    assert!(received.value() == 250);
    ts.return_to_sender(received);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NONCE_ALREADY_USED)]
fun fin_transfer_replay_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);
    call_fin_transfer(&mut state, &mut ts);
    call_fin_transfer(&mut state, &mut ts);
    abort 0
}

#[test]
#[expected_failure]
fun fin_transfer_tampered_signature_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);
    let mut sig = fin_signature();
    *(&mut sig[0]) = 0x00;
    omni_bridge::fin_transfer<TEST_COIN>(
        &mut state,
        sig,
        5,
        1,
        99,
        250,
        USER,
        option::some(string::utf8(b"relayer.near")),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure]
fun fin_transfer_wrong_amount_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);
    omni_bridge::fin_transfer<TEST_COIN>(
        &mut state,
        fin_signature(),
        5,
        1,
        99,
        251, // not what was signed
        USER,
        option::some(string::utf8(b"relayer.near")),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_AMOUNT_OVERFLOW)]
fun fin_transfer_amount_over_u64_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    // Signature over dest_nonce=6, origin_nonce=100, amount=2^64,
    // fee_recipient=None — valid signature, unrepresentable amount.
    let sig = vector[
        0xCD, 0x9E, 0xE7, 0x3E, 0xA4, 0x95, 0x05, 0x17, 0xA4, 0xE0, 0xFE, 0x9C,
        0xB0, 0x4B, 0x63, 0x95, 0x4D, 0xC2, 0xDE, 0x52, 0x77, 0xE4, 0x6F, 0x0E,
        0x66, 0xAE, 0xA8, 0xFF, 0x33, 0x33, 0x22, 0x6D, 0x6E, 0x4C, 0x6C, 0x45,
        0xC0, 0x76, 0x78, 0xF8, 0xF2, 0x10, 0x4E, 0xBD, 0x3E, 0x3F, 0xEE, 0x12,
        0xED, 0x44, 0x18, 0x80, 0x79, 0x7C, 0x51, 0xA9, 0xD1, 0xAD, 0xC8, 0x34,
        0xFA, 0x95, 0x89, 0x7B, 0x1B,
    ];
    omni_bridge::fin_transfer<TEST_COIN>(
        &mut state,
        sig,
        6,
        1,
        100,
        0x10000000000000000, // 2^64
        USER,
        option::none(),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_FIN_TRANSFER_PAUSED)]
fun fin_transfer_paused_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_pause_flags(&mut state, 0x02, ts.ctx());
    call_fin_transfer(&mut state, &mut ts);
    abort 0
}

#[test]
#[expected_failure]
fun fin_transfer_with_wrong_coin_type_aborts() {
    // The payload's token_address is derived from T itself, so submitting
    // the TEST_COIN signature with a different type argument reconstructs
    // different bytes and must fail signature verification.
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);
    omni_bridge::fin_transfer<SUI>(
        &mut state,
        fin_signature(),
        5,
        1,
        99,
        250,
        USER,
        option::some(string::utf8(b"relayer.near")),
        vector[],
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_INITIALIZED)]
fun fin_transfer_unconfigured_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    call_fin_transfer(&mut state, &mut ts);
    abort 0
}

#[test]
fun nonce_bitmap_word_boundaries() {
    let ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::test_mark_nonce_used(&mut state, 127);
    assert!(state.is_transfer_finalised(127));
    assert!(!state.is_transfer_finalised(128));
    assert!(!state.is_transfer_finalised(126));
    omni_bridge::test_mark_nonce_used(&mut state, 128);
    assert!(state.is_transfer_finalised(128));
    // Marking is idempotent.
    omni_bridge::test_mark_nonce_used(&mut state, 128);
    assert!(state.is_transfer_finalised(128));
    assert!(state.is_transfer_finalised(127));
    // Out-of-order distant nonces land in distinct slots.
    omni_bridge::test_mark_nonce_used(&mut state, 1_000_000);
    assert!(state.is_transfer_finalised(1_000_000));
    assert!(!state.is_transfer_finalised(999_999));
    test_scenario::return_shared(state);
    ts.end();
}

// -------- deploy_token / set_token_metadata --------

// Signature over MetadataPayload(token="wrap.testnet", name="Wrapped NEAR",
// symbol="wNEAR", decimals=24), signed by the key behind
// `derived_address()`. Clamped on-chain decimals = 9.
fun deploy_signature(): vector<u8> {
    vector[
        0xEA, 0xF3, 0x60, 0x57, 0xBF, 0xCF, 0x7D, 0x5F, 0x95, 0x64, 0xFB, 0x00,
        0x2A, 0xDF, 0x73, 0x1F, 0xCD, 0x65, 0x29, 0xED, 0xDB, 0x0A, 0xB9, 0x10,
        0xAD, 0xC1, 0x88, 0xD8, 0x47, 0xCF, 0xDD, 0xE3, 0x6C, 0x83, 0x68, 0xC3,
        0xB2, 0x7B, 0x2C, 0x0C, 0xA1, 0x1F, 0x0F, 0xF3, 0x07, 0x8F, 0x5F, 0x3B,
        0x81, 0xBB, 0xFA, 0xAF, 0x25, 0x04, 0xE6, 0xD9, 0xB4, 0x39, 0x50, 0x42,
        0xD9, 0xB1, 0x98, 0x75, 0x1C,
    ]
}

/// TreasuryCap + CoinMetadata matching the signed payload (decimals
/// pre-clamped to 9) + an UpgradeCap for TEST_COIN's defining package
/// (`omni_bridge` == @0x0 in unit tests) at version 1.
fun deploy_fixtures(
    ts: &mut Scenario,
): (
    coin::TreasuryCap<TEST_COIN>,
    coin::CoinMetadata<TEST_COIN>,
    sui::package::UpgradeCap,
) {
    let (cap, metadata) = test_coin::create_currency(
        9,
        b"wNEAR",
        b"Wrapped NEAR",
        ts.ctx(),
    );
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    (cap, metadata, upgrade_cap)
}

fun call_deploy_token(state: &mut BridgeState, ts: &mut Scenario) {
    let (cap, metadata, upgrade_cap) = deploy_fixtures(ts);
    omni_bridge::deploy_token<TEST_COIN>(
        state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
}

#[test]
fun deploy_token_registers_bridge_token() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);

    assert!(state.is_bridge_token<TEST_COIN>());
    assert!(
        state.get_token_address(string::utf8(b"wrap.testnet"))
            == option::some(utils::coin_type_string<TEST_COIN>()),
    );
    assert!(
        state.get_coin_type(utils::token_address<TEST_COIN>())
            == option::some(utils::coin_type_string<TEST_COIN>()),
    );

    let events = event::events_by_type<omni_bridge::DeployToken>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_deploy_token_event(
        utils::token_address<TEST_COIN>(),
        utils::coin_type_string<TEST_COIN>(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        9, // clamped
        24, // origin
    );
    assert!(events[0] == expected);

    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun bridged_token_mints_on_fin_and_burns_on_init() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);

    // fin_transfer mints (no custody involved).
    call_fin_transfer(&mut state, &mut ts);
    assert!(state.locked_balance<TEST_COIN>() == 0);

    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let minted = ts.take_from_address<coin::Coin<TEST_COIN>>(USER);
    assert!(minted.value() == 250);

    // init_transfer burns the bridged token instead of locking it.
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::init_transfer(
        &mut state,
        minted,
        0,
        coin::zero<SUI>(ts.ctx()),
        string::utf8(b"near:bob.near"),
        vector[],
        ts.ctx(),
    );
    assert!(state.locked_balance<TEST_COIN>() == 0);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_TYPE_ALREADY_USED)]
fun deploy_token_same_type_for_second_near_token_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    // Signature over MetadataPayload(token="usdt.testnet", same
    // name/symbol/decimals) — a different NEAR token id must not be
    // bindable to the already-used coin type.
    let usdt_sig = vector[
        0x8D, 0xB7, 0x79, 0x62, 0x0B, 0x57, 0x86, 0xDA, 0xE5, 0xA0, 0x9F, 0x0F,
        0x94, 0x93, 0x47, 0x57, 0x01, 0x0F, 0x8F, 0x07, 0x74, 0x8A, 0xBA, 0x59,
        0xFA, 0xD3, 0x41, 0xB6, 0x46, 0x5F, 0xC3, 0xC8, 0x72, 0xA5, 0xA3, 0x6C,
        0x7D, 0x6F, 0x73, 0x4C, 0xC2, 0xBA, 0x41, 0x11, 0x2C, 0x21, 0x00, 0x92,
        0x2A, 0x3C, 0xF8, 0x4F, 0x07, 0x86, 0x90, 0x20, 0xB4, 0xA9, 0x62, 0x1E,
        0xC3, 0xA3, 0xA0, 0xA9, 0x1C,
    ];
    let (cap, metadata, upgrade_cap) = deploy_fixtures(&mut ts);
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        usdt_sig,
        string::utf8(b"usdt.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_UPGRADE_CAP)]
fun deploy_token_upgraded_cap_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(
        9,
        b"wNEAR",
        b"Wrapped NEAR",
        ts.ctx(),
    );
    // Simulate a completed package upgrade: cap version becomes 2 (and the
    // package id moves to the new version's id). `authorize_upgrade`
    // reserves package id 0x0 as its already-authorized sentinel, so the
    // cap must start from a non-zero id here; either failing conjunct of
    // the upgrade-cap check yields E_INVALID_UPGRADE_CAP.
    let mut upgrade_cap = sui::package::test_publish(
        object::id_from_address(@0xABC),
        ts.ctx(),
    );
    let ticket = sui::package::authorize_upgrade(
        &mut upgrade_cap,
        sui::package::compatible_policy(),
        vector[0x01],
    );
    let receipt = sui::package::test_upgrade(ticket);
    sui::package::commit_upgrade(&mut upgrade_cap, receipt);
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_INITIALIZED)]
fun deploy_token_unconfigured_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_TOKEN_ALREADY_DEPLOYED)]
fun deploy_token_twice_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    call_deploy_token(&mut state, &mut ts);
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_SUPPLY_NOT_ZERO)]
fun deploy_token_nonzero_supply_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (mut cap, metadata, upgrade_cap) = deploy_fixtures(&mut ts);
    let premint = coin::mint(&mut cap, 1, ts.ctx());
    transfer::public_transfer(premint, ADMIN);
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_UPGRADE_CAP)]
fun deploy_token_wrong_upgrade_cap_package_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(
        9,
        b"wNEAR",
        b"Wrapped NEAR",
        ts.ctx(),
    );
    let wrong = sui::package::test_publish(object::id_from_address(@0xBEEF), ts.ctx());
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        wrong,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_decimals_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    // Coin published with 6 decimals; the signed payload clamps 24 -> 9.
    let (cap, metadata) = test_coin::create_currency(
        6,
        b"wNEAR",
        b"Wrapped NEAR",
        ts.ctx(),
    );
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_name_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(
        9,
        b"wNEAR",
        b"Wrong Name",
        ts.ctx(),
    );
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_symbol_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(
        9,
        b"EVIL",
        b"Wrapped NEAR",
        ts.ctx(),
    );
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure]
fun deploy_token_tampered_signature_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata, upgrade_cap) = deploy_fixtures(&mut ts);
    let mut sig = deploy_signature();
    *(&mut sig[0]) = 0x00;
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        sig,
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
        cap,
        upgrade_cap,
        metadata,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_DEPLOY_TOKEN_PAUSED)]
fun deploy_token_paused_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_pause_flags(&mut state, 0x04, ts.ctx());
    call_deploy_token(&mut state, &mut ts);
    abort 0
}

#[test]
fun metadata_admin_updates_token_metadata() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"Bridged wNEAR")),
        option::some(std::ascii::string(b"https://example.com/wnear.png")),
        ts.ctx(),
    );
    let events = event::events_by_type<omni_bridge::TokenMetadataChanged>();
    assert!(events.length() == 1);

    // Both fields land in the stored CoinMetadata.
    assert!(
        omni_bridge::test_token_description<TEST_COIN>(&state)
            == string::utf8(b"Bridged wNEAR"),
    );
    assert!(
        omni_bridge::test_token_icon_url<TEST_COIN>(&state)
            == option::some(sui::url::new_unsafe_from_bytes(b"https://example.com/wnear.png")),
    );

    // `None` leaves a field unchanged.
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"v2")),
        option::none(),
        ts.ctx(),
    );
    assert!(omni_bridge::test_token_description<TEST_COIN>(&state) == string::utf8(b"v2"));
    assert!(
        omni_bridge::test_token_icon_url<TEST_COIN>(&state)
            == option::some(sui::url::new_unsafe_from_bytes(b"https://example.com/wnear.png")),
    );

    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_INITIALIZED)]
fun log_metadata_unconfigured_aborts() {
    let mut ts = setup();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(6, b"TST", b"Test Coin", ts.ctx());
    omni_bridge::log_metadata(&mut state, &metadata);
    std::unit_test::destroy(cap);
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun revoke_role_by_non_admin_aborts() {
    let mut ts = setup();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::revoke_role(&mut state, ROLE_ADMIN, ADMIN, ts.ctx());
    abort 0
}

#[test]
fun set_token_metadata_updates_description() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"Bridged wNEAR")),
        option::none(),
        ts.ctx(),
    );
    assert!(
        omni_bridge::test_token_description<TEST_COIN>(&state)
            == string::utf8(b"Bridged wNEAR"),
    );
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun granted_metadata_admin_can_set_metadata() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    omni_bridge::grant_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"by user")),
        option::none(),
        ts.ctx(),
    );
    assert!(
        omni_bridge::test_token_description<TEST_COIN>(&state)
            == string::utf8(b"by user"),
    );
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun revoked_metadata_admin_cannot_set_metadata() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    omni_bridge::grant_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    omni_bridge::revoke_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"x")),
        option::none(),
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun set_token_metadata_by_non_admin_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"x")),
        option::none(),
        ts.ctx(),
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NOT_BRIDGE_TOKEN)]
fun set_token_metadata_on_non_bridge_token_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        option::some(string::utf8(b"x")),
        option::none(),
        ts.ctx(),
    );
    abort 0
}

// -------- log_metadata --------

#[test]
fun log_metadata_emits_and_registers() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (cap, metadata) = test_coin::create_currency(6, b"TST", b"Test Coin", ts.ctx());

    omni_bridge::log_metadata(&mut state, &metadata);

    let events = event::events_by_type<omni_bridge::LogMetadata>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_log_metadata_event(
        utils::token_address<TEST_COIN>(),
        utils::coin_type_string<TEST_COIN>(),
        string::utf8(b"Test Coin"),
        string::utf8(b"TST"),
        6,
    );
    assert!(events[0] == expected);

    // Reverse registry is populated (idempotently).
    omni_bridge::log_metadata(&mut state, &metadata);
    let coin_type = state.get_coin_type(utils::token_address<TEST_COIN>());
    assert!(coin_type == option::some(utils::coin_type_string<TEST_COIN>()));

    std::unit_test::destroy(cap);
    std::unit_test::destroy(metadata);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun log_metadata_registry_emits_and_registers() {
    let mut ts = setup_configured();
    // Build a coin_registry::Currency<TEST_COIN> from legacy metadata via
    // the framework's test-only helpers (registry creation requires the
    // system address).
    let (cap, metadata) = test_coin::create_currency(6, b"TST", b"Test Coin", ts.ctx());
    ts.next_tx(@0x0);
    let mut registry = sui::coin_registry::create_coin_data_registry_for_testing(ts.ctx());
    let currency = sui::coin_registry::migrate_legacy_metadata_for_testing(
        &mut registry,
        &metadata,
        ts.ctx(),
    );
    ts.next_tx(ADMIN);

    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::log_metadata_registry(&mut state, &currency);

    let events = event::events_by_type<omni_bridge::LogMetadata>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_log_metadata_event(
        utils::token_address<TEST_COIN>(),
        utils::coin_type_string<TEST_COIN>(),
        string::utf8(b"Test Coin"),
        string::utf8(b"TST"),
        6,
    );
    assert!(events[0] == expected);
    assert!(
        state.get_coin_type(utils::token_address<TEST_COIN>())
            == option::some(utils::coin_type_string<TEST_COIN>()),
    );

    std::unit_test::destroy(cap);
    std::unit_test::destroy(metadata);
    std::unit_test::destroy(currency);
    std::unit_test::destroy(registry);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun get_coin_type_unknown_is_none() {
    let ts = setup_configured();
    let state = ts.take_shared<BridgeState>();
    assert!(state.get_coin_type(@0xDEAD).is_none());
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun rotate_derived_address_by_non_admin_aborts() {
    let mut ts = setup_configured();
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_near_bridge_derived_address(&mut state, derived_address(), ts.ctx());
    abort 0
}
