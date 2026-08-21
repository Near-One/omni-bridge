#[test_only]
module omni_bridge::omni_bridge_tests;

use omni_bridge::omni_bridge::{Self, BridgeState, TokenSetup};
use omni_bridge::test_coin::{Self, TEST_COIN};
use omni_bridge::token::{Self, TOKEN};
use omni_bridge::utils;
use std::string;
use sui::coin;
use sui::coin_registry::Currency;
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
        0x2C, 0x75, 0x36, 0xE3, 0x60, 0x5D, 0x9C, 0x16, 0xA7, 0xA3, 0xD7, 0xB1,
        0x89, 0x8E, 0x52, 0x93, 0x96, 0xA6, 0x5C, 0x23,
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
        0xF4, 0xFE, 0x60, 0xAF, 0x38, 0x50, 0xE4, 0xC7, 0xDD, 0xDA, 0xE2, 0xB8,
        0xB4, 0x1E, 0x73, 0x84, 0x88, 0x2D, 0x60, 0x47, 0x3D, 0xFB, 0x8D, 0x0B,
        0x3F, 0x88, 0x0F, 0x00, 0x89, 0x8F, 0x6F, 0xE5, 0x47, 0x9A, 0xD2, 0x32,
        0xED, 0x59, 0xED, 0xA4, 0x03, 0xE3, 0x6A, 0x81, 0xFE, 0xE3, 0x26, 0x4A,
        0x2A, 0xE2, 0x23, 0xF1, 0x7C, 0xED, 0x32, 0x12, 0xB7, 0x4D, 0xC3, 0x52,
        0x2C, 0x1F, 0x52, 0x90, 0x1C,
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
#[expected_failure(abort_code = utils::E_INVALID_SIGNATURE)]
fun fin_transfer_wrong_signer_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    lock_some_test_coin(&mut state, 1_000, &mut ts);
    // Valid signature over the right payload from the wrong key: ecrecover
    // succeeds, so the address comparison is what must reject it.
    let wrong_signer_sig = vector[
        0xF1, 0x30, 0xE7, 0x6C, 0x20, 0x33, 0x59, 0xB4, 0xE3, 0x3C, 0x34, 0x0F,
        0x0C, 0x47, 0x17, 0x5F, 0xDF, 0xB6, 0x8C, 0x14, 0x4D, 0xD2, 0xFD, 0x35,
        0x13, 0xB3, 0x7E, 0x6E, 0x5B, 0x4E, 0x83, 0x60, 0x21, 0xE3, 0x02, 0xA7,
        0xDD, 0x77, 0x3E, 0x79, 0x13, 0xEE, 0xCF, 0x75, 0x4A, 0x97, 0xA1, 0x27,
        0x85, 0xFE, 0xC8, 0xFD, 0x06, 0x20, 0x8B, 0x1C, 0x2A, 0x1D, 0x40, 0xB5,
        0x11, 0x8B, 0xFA, 0xCD, 0x1C,
    ];
    omni_bridge::fin_transfer<TEST_COIN>(
        &mut state,
        wrong_signer_sig,
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
#[expected_failure(abort_code = 0, location = sui::ecdsa_k1)]
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
#[expected_failure(abort_code = utils::E_INVALID_SIGNATURE)]
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
        0xD0, 0xEC, 0x56, 0xC8, 0x07, 0xAF, 0x43, 0xE0, 0xD6, 0xD7, 0x22, 0xD5,
        0x83, 0x40, 0x44, 0x82, 0x2F, 0xA4, 0x77, 0x25, 0x54, 0x9A, 0x4A, 0x4F,
        0x44, 0xC7, 0x2C, 0x2F, 0x48, 0x89, 0x38, 0x85, 0x69, 0x05, 0x1C, 0x29,
        0xAE, 0xB9, 0xC7, 0x14, 0xE4, 0x56, 0xEC, 0x9A, 0xA3, 0x50, 0x14, 0xB8,
        0x13, 0xC4, 0x6D, 0xDF, 0x67, 0x6D, 0x30, 0x0B, 0x1E, 0x5A, 0xEA, 0x1E,
        0x4B, 0x0C, 0x6A, 0xD9, 0x1C,
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
#[expected_failure(abort_code = utils::E_INVALID_SIGNATURE)]
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
        0x78, 0xAD, 0x2C, 0xF5, 0x84, 0xC1, 0xB6, 0x3D, 0x45, 0xEA, 0x67, 0x9F,
        0x67, 0xB1, 0xC7, 0x09, 0x9F, 0x4D, 0x28, 0x76, 0xF0, 0x75, 0xA4, 0xF4,
        0x2B, 0xA6, 0xB3, 0xC3, 0xEE, 0x0F, 0xEF, 0x7F, 0x6A, 0xAB, 0x0D, 0x7B,
        0x9F, 0x99, 0xB0, 0x96, 0x33, 0x3D, 0x74, 0x66, 0x36, 0x62, 0x53, 0x0D,
        0x08, 0x99, 0xEC, 0xDC, 0xFB, 0xE6, 0xDE, 0x7A, 0x0E, 0x19, 0x15, 0x8B,
        0x74, 0xB6, 0x32, 0xF3, 0x1B,
    ]
}

/// Metadata matching the signed payload, plus a fresh `UpgradeCap` for
/// TOKEN's defining package (`omni_bridge` == @0x0 under `sui move test`).
fun deploy_fixtures(ts: &mut Scenario): (TokenSetup<TOKEN>, sui::package::UpgradeCap) {
    let setup = token::prepare(9, b"wNEAR", b"Wrapped NEAR", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    (setup, upgrade_cap)
}

fun call_deploy_token(state: &mut BridgeState, ts: &mut Scenario) {
    let (setup, upgrade_cap) = deploy_fixtures(ts);
    omni_bridge::deploy_token<TOKEN>(
        state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
}

// As `fin_signature()` but over TOKEN's type id, so it mints rather than
// unlocks.
fun fin_signature_token(): vector<u8> {
    vector[
        0xA1, 0xB2, 0x6D, 0x6E, 0x56, 0x1B, 0x6F, 0xE5, 0xB5, 0x0D, 0x10, 0xB1,
        0x4B, 0x5C, 0x27, 0x89, 0xB4, 0xA0, 0x4B, 0x0E, 0x8E, 0x6A, 0x78, 0x2C,
        0x89, 0x35, 0x25, 0x0D, 0x3D, 0xC3, 0x43, 0xB7, 0x44, 0x5A, 0x2F, 0x5C,
        0x24, 0x2C, 0x9E, 0xBA, 0x55, 0x40, 0x21, 0x34, 0x29, 0xA3, 0x8B, 0xF1,
        0xC4, 0x76, 0x66, 0x87, 0x02, 0xC0, 0xD3, 0x10, 0x5D, 0x72, 0xBE, 0x15,
        0x18, 0x0B, 0x8C, 0x27, 0x1B,
    ]
}

fun call_fin_transfer_token(state: &mut BridgeState, ts: &mut Scenario) {
    omni_bridge::fin_transfer<TOKEN>(
        state,
        fin_signature_token(),
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

#[test]
fun deploy_token_registers_bridge_token() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);

    assert!(state.is_bridge_token<TOKEN>());
    assert!(omni_bridge::test_has_metadata_cap<TOKEN>(&state));
    assert!(
        state.get_token_address(string::utf8(b"wrap.testnet"))
            == option::some(utils::coin_type_string<TOKEN>()),
    );
    assert!(
        state.get_coin_type(utils::token_address<TOKEN>())
            == option::some(utils::coin_type_string<TOKEN>()),
    );

    let events = event::events_by_type<omni_bridge::DeployToken>();
    assert!(events.length() == 1);
    let expected = omni_bridge::new_deploy_token_event(
        utils::token_address<TOKEN>(),
        utils::coin_type_string<TOKEN>(),
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
fun prepared_currency_is_unregulated_with_signed_metadata() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);
    test_scenario::return_shared(state);
    ts.next_tx(ADMIN);

    // `prepare_token` parks the Currency at the CoinRegistry address.
    let currency = ts.take_from_address<Currency<TOKEN>>(@0xc);
    // The point of the two-step flow: no DenyCapV2 exists or can be minted.
    assert!(!currency.is_regulated());
    assert!(currency.deny_cap_id() == option::none());
    assert!(currency.symbol() == string::utf8(b"wNEAR"));
    assert!(currency.name() == string::utf8(b"Wrapped NEAR"));
    assert!(currency.decimals() == 9);
    test_scenario::return_to_address(@0xc, currency);
    ts.end();
}

#[test]
fun bridged_token_mints_on_fin_and_burns_on_init() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, &mut ts);

    // fin_transfer mints (no custody involved).
    call_fin_transfer_token(&mut state, &mut ts);
    assert!(state.locked_balance<TOKEN>() == 0);

    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let minted = ts.take_from_address<coin::Coin<TOKEN>>(USER);
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
    assert!(state.locked_balance<TOKEN>() == 0);
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
        0xC1, 0x52, 0x30, 0x34, 0x7B, 0x3A, 0xA8, 0xD7, 0xE3, 0x82, 0xE6, 0x96,
        0x70, 0x42, 0x3A, 0xDF, 0xDD, 0x4A, 0x5C, 0xCA, 0x1A, 0x6C, 0xC5, 0xF3,
        0x8B, 0x86, 0x28, 0x0F, 0xA6, 0x90, 0xE2, 0x10, 0x74, 0x27, 0x3F, 0x0E,
        0x0F, 0xE1, 0x7F, 0xC9, 0xF7, 0x94, 0xEA, 0x97, 0xA6, 0xD6, 0xF2, 0x8E,
        0x55, 0xF8, 0xA0, 0xF7, 0xF5, 0xAE, 0xB0, 0x47, 0xB8, 0xDD, 0xC0, 0x7A,
        0x4A, 0x10, 0x08, 0x07, 0x1C,
    ];
    let (setup, upgrade_cap) = deploy_fixtures(&mut ts);
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        usdt_sig,
        string::utf8(b"usdt.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_UPGRADE_CAP)]
fun deploy_token_upgraded_cap_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let setup = token::prepare(9, b"wNEAR", b"Wrapped NEAR", ts.ctx());
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
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_UPGRADE_CAP)]
fun deploy_token_wrong_upgrade_cap_package_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let setup = token::prepare(9, b"wNEAR", b"Wrapped NEAR", ts.ctx());
    let wrong = sui::package::test_publish(object::id_from_address(@0xBEEF), ts.ctx());
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        wrong,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
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

// No `E_SUPPLY_NOT_ZERO` test: `prepare_token` mints the cap and it stays
// sealed in `TokenSetup`, so pre-minting is unreachable from outside.

#[test]
#[expected_failure(abort_code = omni_bridge::E_SETUP_VERSION_MISMATCH)]
fun deploy_token_stale_setup_version_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (mut setup, upgrade_cap) = deploy_fixtures(&mut ts);
    omni_bridge::test_set_setup_version(&mut setup, 0);
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_NON_CANONICAL_COIN_TYPE)]
fun deploy_token_non_canonical_type_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    // Correct in every respect but the type name.
    let setup = test_coin::prepare_non_canonical(9, b"wNEAR", b"Wrapped NEAR", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TEST_COIN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_DECIMALS_TOO_LARGE)]
fun prepare_token_decimals_above_cap_aborts() {
    let mut ts = setup_configured();
    // The template must publish with min(origin_decimals, 9); passing the
    // unclamped value fails at publish rather than wasting the package.
    let setup = token::prepare(24, b"wNEAR", b"Wrapped NEAR", ts.ctx());
    std::unit_test::destroy(setup);
    abort 0
}

#[test]
fun deploy_token_below_clamp_keeps_origin_decimals() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    // Below the clamp, so decimals must pass through unchanged.
    let six_sig = vector[
        0xF8, 0x59, 0xCA, 0x6C, 0xE1, 0x82, 0x2F, 0xB6, 0x69, 0x0D, 0x46, 0x0C,
        0xB0, 0xC2, 0xA7, 0xB0, 0x6E, 0xFE, 0x3B, 0x05, 0x2C, 0x0F, 0x12, 0x0C,
        0x68, 0x9B, 0x11, 0xB7, 0xA1, 0x7A, 0x87, 0x21, 0x08, 0x73, 0x97, 0xD1,
        0x02, 0xB3, 0xCA, 0x3A, 0xDB, 0x78, 0xAA, 0x41, 0x63, 0xE0, 0xBA, 0x50,
        0xF3, 0x14, 0x8A, 0x63, 0x68, 0x5A, 0x1F, 0x1D, 0x85, 0x21, 0xF3, 0xD8,
        0x03, 0x31, 0x81, 0x31, 0x1C,
    ];
    let setup = token::prepare(6, b"SIX", b"Six Dec", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        six_sig,
        string::utf8(b"six.testnet"),
        string::utf8(b"Six Dec"),
        string::utf8(b"SIX"),
        6,
    );
    let events = event::events_by_type<omni_bridge::DeployToken>();
    let expected = omni_bridge::new_deploy_token_event(
        utils::token_address<TOKEN>(),
        utils::coin_type_string<TOKEN>(),
        string::utf8(b"six.testnet"),
        string::utf8(b"Six Dec"),
        string::utf8(b"SIX"),
        6, // unclamped
        6, // origin
    );
    assert!(events[0] == expected);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_INVALID_SYMBOL)]
fun prepare_token_non_ascii_symbol_aborts() {
    let mut ts = setup_configured();
    // NEAR would sign this symbol; Sui cannot represent it.
    let setup = token::prepare(9, b"w\xE2\x82\xACNEAR", b"Wrapped NEAR", ts.ctx());
    std::unit_test::destroy(setup);
    abort 0
}

#[test]
fun cancel_token_setup_releases_caps() {
    let mut ts = setup_configured();
    // Metadata that can never match a payload: caps must be recoverable.
    let setup = token::prepare(9, b"WRONG", b"Wrong Name", ts.ctx());
    let (cap, metadata_cap) = omni_bridge::cancel_token_setup(setup);
    assert!(coin::total_supply(&cap) == 0);
    std::unit_test::destroy(cap);
    std::unit_test::destroy(metadata_cap);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_decimals_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    // Coin published with 6 decimals; the signed payload clamps 24 -> 9.
    let setup = token::prepare(6, b"wNEAR", b"Wrapped NEAR", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_name_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let setup = token::prepare(9, b"wNEAR", b"Wrong Name", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_METADATA_MISMATCH)]
fun deploy_token_wrong_symbol_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let setup = token::prepare(9, b"EVIL", b"Wrapped NEAR", ts.ctx());
    let upgrade_cap = sui::package::test_publish(object::id_from_address(@omni_bridge), ts.ctx());
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        deploy_signature(),
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
    );
    abort 0
}

#[test]
#[expected_failure(abort_code = utils::E_INVALID_SIGNATURE)]
fun deploy_token_tampered_signature_aborts() {
    let mut ts = setup_configured();
    let mut state = ts.take_shared<BridgeState>();
    let (setup, upgrade_cap) = deploy_fixtures(&mut ts);
    let mut sig = deploy_signature();
    *(&mut sig[0]) = 0x00;
    omni_bridge::deploy_token<TOKEN>(
        &mut state,
        setup,
        upgrade_cap,
        sig,
        string::utf8(b"wrap.testnet"),
        string::utf8(b"Wrapped NEAR"),
        string::utf8(b"wNEAR"),
        24,
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

/// Deploy, then take the state and the coin's `Currency` in a fresh tx.
fun deployed_with_currency(ts: &mut Scenario): (BridgeState, Currency<TOKEN>) {
    let mut state = ts.take_shared<BridgeState>();
    call_deploy_token(&mut state, ts);
    test_scenario::return_shared(state);
    ts.next_tx(ADMIN);
    (ts.take_shared<BridgeState>(), ts.take_from_address<Currency<TOKEN>>(@0xc))
}

#[test]
fun metadata_admin_updates_token_metadata() {
    let mut ts = setup_configured();
    let (mut state, mut currency) = deployed_with_currency(&mut ts);
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
        option::some(string::utf8(b"Bridged wNEAR")),
        option::some(string::utf8(b"https://example.com/wnear.png")),
        ts.ctx(),
    );
    let events = event::events_by_type<omni_bridge::TokenMetadataChanged>();
    assert!(events.length() == 1);

    assert!(currency.description() == string::utf8(b"Bridged wNEAR"));
    assert!(currency.icon_url() == string::utf8(b"https://example.com/wnear.png"));

    // `None` leaves a field unchanged.
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
        option::some(string::utf8(b"v2")),
        option::none(),
        ts.ctx(),
    );
    assert!(currency.description() == string::utf8(b"v2"));
    assert!(currency.icon_url() == string::utf8(b"https://example.com/wnear.png"));

    // Fixed by the signed payload; no setter exists.
    assert!(currency.name() == string::utf8(b"Wrapped NEAR"));
    assert!(currency.symbol() == string::utf8(b"wNEAR"));

    test_scenario::return_to_address(@0xc, currency);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun set_token_metadata_updates_description() {
    let mut ts = setup_configured();
    let (mut state, mut currency) = deployed_with_currency(&mut ts);
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
        option::some(string::utf8(b"Bridged wNEAR")),
        option::none(),
        ts.ctx(),
    );
    assert!(currency.description() == string::utf8(b"Bridged wNEAR"));
    test_scenario::return_to_address(@0xc, currency);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
fun granted_metadata_admin_can_set_metadata() {
    let mut ts = setup_configured();
    let (mut state, mut currency) = deployed_with_currency(&mut ts);
    omni_bridge::grant_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
        option::some(string::utf8(b"by user")),
        option::none(),
        ts.ctx(),
    );
    assert!(currency.description() == string::utf8(b"by user"));
    test_scenario::return_to_address(@0xc, currency);
    test_scenario::return_shared(state);
    ts.end();
}

#[test]
#[expected_failure(abort_code = omni_bridge::E_UNAUTHORIZED)]
fun revoked_metadata_admin_cannot_set_metadata() {
    let mut ts = setup_configured();
    let (mut state, mut currency) = deployed_with_currency(&mut ts);
    omni_bridge::grant_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    omni_bridge::revoke_role(&mut state, ROLE_METADATA_ADMIN, USER, ts.ctx());
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
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
    let (state, mut currency) = deployed_with_currency(&mut ts);
    test_scenario::return_shared(state);
    ts.next_tx(USER);
    let mut state = ts.take_shared<BridgeState>();
    omni_bridge::set_token_metadata<TOKEN>(
        &mut state,
        &mut currency,
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
    let state = ts.take_shared<BridgeState>();
    // Prepared but never deployed: no MetadataCap in custody.
    let setup = test_coin::prepare_non_canonical(9, b"TST", b"Test Coin", ts.ctx());
    std::unit_test::destroy(setup);
    test_scenario::return_shared(state);
    ts.next_tx(ADMIN);
    let mut state = ts.take_shared<BridgeState>();
    let mut currency = ts.take_from_address<Currency<TEST_COIN>>(@0xc);
    omni_bridge::set_token_metadata<TEST_COIN>(
        &mut state,
        &mut currency,
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
