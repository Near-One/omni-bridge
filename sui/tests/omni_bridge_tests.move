#[test_only]
module omni_bridge::omni_bridge_tests;

use omni_bridge::{
    omni_bridge::{Self, BridgeState, TokenSetup},
    test_coin::{Self, TEST_COIN},
    token::{Self, TOKEN},
    utils
};
use std::string;
use sui::{coin, coin_registry::Currency, event, sui::SUI, test_scenario::{Self, Scenario}};

const ADMIN: address = @0xAD;
const USER: address = @0xB0B;
const CHAIN_ID: u8 = 14;

const ROLE_ADMIN: u8 = 0;
const ROLE_PAUSER: u8 = 1;
const ROLE_METADATA_ADMIN: u8 = 2;

fun derived_address(): vector<u8> {
    x"2C7536E3605D9C16A7A3D7B1898E529396A65C23"
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
    x"F4FE60AF3850E4C7DDDAE2B8B41E7384882D60473DFB8D0B3F880F00898F6FE5479AD232ED59EDA403E36A81FEE3264A2AE223F17CED3212B74DC3522C1F52901C"
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
    let wrong_signer_sig =
        x"F130E76C203359B4E33C340F0C47175FDFB68C144DD2FD3513B37E6E5B4E836021E302A7DD773E7913EECF754A97A12785FEC8FD06208B1C2A1D40B5118BFACD1C";
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
    let sig =
        x"D0EC56C807AF43E0D6D722D5834044822FA47725549A4A4F44C72C2F4889388569051C29AEB9C714E456EC9AA35014B813C46DDF676D300B1E5AEA1E4B0C6AD91C";
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
    x"78AD2CF584C1B63D45EA679F67B1C7099F4D2876F075A4F42BA6B3C3EE0FEF7F6AAB0D7B9F99B096333D74663662530D0899ECDCFBE6DE7A0E19158B74B632F31B"
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
    x"A1B26D6E561B6FE5B50D10B14B5C2789B4A04B0E8E6A782C8935250D3DC343B7445A2F5C242C9EBA5540213429A38BF1C476668702C0D3105D72BE15180B8C271B"
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
    let usdt_sig =
        x"C15230347B3AA8D7E382E69670423ADFDD4A5CCA1A6CC5F38B86280FA690E21074273F0E0FE17FC9F794EA97A6D6F28E55F8A0F7F5AEB047B8DDC07A4A1008071C";
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
    let six_sig =
        x"F859CA6CE1822FB6690D460CB0C2A7B06EFE3B052C0F120C689B11B7A17A8721087397D102B3CA3ADB78AA4163E0BA50F3148A63685A1F1D8521F3D8033181311C";
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
