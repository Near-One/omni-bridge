#[test_only]
module omni_bridge::omni_bridge_tests {
    use std::option;
    use std::string;
    use aptos_framework::account;
    use aptos_framework::fungible_asset::{Self, Metadata, MintRef};
    use aptos_framework::object::{Self, Object};
    use aptos_framework::primary_fungible_store;

    use omni_bridge::bridge_token;
    use omni_bridge::bridge_types;
    use omni_bridge::omni_bridge;
    use omni_bridge::utils;

    /// Create a stand-alone FA (not bridge-deployed) usable as `native_token_metadata`
    /// or as the "locked token" path in tests. The `MintRef` is not exposed —
    /// use `create_test_fa_with_mint` if the test needs to mint.
    fun create_test_fa(creator: &signer, seed: vector<u8>, decimals: u8): Object<Metadata> {
        let constructor_ref = object::create_named_object(creator, seed);
        primary_fungible_store::create_primary_store_enabled_fungible_asset(
            &constructor_ref,
            option::none(),
            string::utf8(b"TestToken"),
            string::utf8(b"TT"),
            decimals,
            string::utf8(b""),
            string::utf8(b"")
        );
        constructor_ref.object_from_constructor_ref<Metadata>()
    }

    /// Same as `create_test_fa` but also returns a `MintRef` so the test
    /// can mint tokens to user accounts. Used by `init_transfer` tests.
    fun create_test_fa_with_mint(
        creator: &signer, seed: vector<u8>, decimals: u8
    ): (Object<Metadata>, MintRef) {
        let constructor_ref = object::create_named_object(creator, seed);
        primary_fungible_store::create_primary_store_enabled_fungible_asset(
            &constructor_ref,
            option::none(),
            string::utf8(b"TestToken"),
            string::utf8(b"TT"),
            decimals,
            string::utf8(b""),
            string::utf8(b"")
        );
        let mint_ref = fungible_asset::generate_mint_ref(&constructor_ref);
        let metadata = constructor_ref.object_from_constructor_ref<Metadata>();
        (metadata, mint_ref)
    }

    /// Stash the mint ref on a throwaway object so it can be "dropped"
    /// after a test mints what it needs (`MintRef` has no `drop` ability).
    struct MintRefHolder has key {
        mint_ref: MintRef
    }

    fun stash_mint_ref(
        creator: &signer, seed: vector<u8>, mint_ref: MintRef
    ) {
        let ref_obj = object::create_named_object(creator, seed);
        let obj_signer = ref_obj.generate_signer();
        move_to(&obj_signer, MintRefHolder { mint_ref });
    }

    fun mint_to(mint_ref: &MintRef, recipient: address, amount: u64) {
        let fa = fungible_asset::mint(mint_ref, amount);
        primary_fungible_store::deposit(recipient, fa);
    }

    fun setup(deployer: &signer): Object<Metadata> {
        account::create_account_for_test(deployer.address_of());
        let native_metadata = create_test_fa(deployer, b"NATIVE", 8);
        // 20-byte placeholder near_bridge_derived_address.
        let derived = vector[];
        for (i in 0..20) {
            derived.push_back((i as u8));
        };
        omni_bridge::test_initialize(deployer, derived, 13u8, native_metadata);
        native_metadata
    }

    /// Resolve a role id by name via the on-chain registry. Aborts if the
    /// name doesn't exist — same error the production code would surface.
    fun role_id(name: vector<u8>): u8 {
        let target = string::utf8(name);
        let roles = omni_bridge::all_roles();
        let i = 0;
        while (i < roles.length()) {
            let role = &roles[i];
            if (omni_bridge::role_info_name(role) == target) {
                return omni_bridge::role_info_id(role)
            };
            i += 1;
        };
        abort 0
    }

    #[test(deployer = @omni_bridge)]
    fun initializes_with_zero_nonce_and_no_pause(deployer: signer) {
        let _native = setup(&deployer);
        assert!(omni_bridge::current_origin_nonce() == 0, 200);
        assert!(omni_bridge::pause_flags() == 0, 201);
        assert!(omni_bridge::chain_id() == 13, 202);
        assert!(!omni_bridge::is_transfer_finalised(0), 203);
        assert!(!omni_bridge::is_transfer_finalised(1234), 204);
    }

    #[test(deployer = @omni_bridge)]
    #[expected_failure(abort_code = 1, location = omni_bridge::omni_bridge)]
    fun cannot_initialize_twice(deployer: signer) {
        let _ = setup(&deployer);
        let derived = vector[];
        for (_i in 0..20) {
            derived.push_back(0u8);
        };
        let other = create_test_fa(&deployer, b"ANOTHER", 8);
        omni_bridge::test_initialize(&deployer, derived, 13u8, other);
    }

    #[test(deployer = @omni_bridge, attacker = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun set_pause_flags_requires_admin(
        deployer: signer, attacker: signer
    ) {
        let _ = setup(&deployer);
        account::create_account_for_test(attacker.address_of());
        omni_bridge::set_pause_flags(&attacker, 0x01);
    }

    #[test(deployer = @omni_bridge)]
    fun admin_can_pause_and_unpause(deployer: signer) {
        let _ = setup(&deployer);
        omni_bridge::set_pause_flags(&deployer, 0x01);
        assert!(omni_bridge::pause_flags() == 0x01, 210);
        omni_bridge::set_pause_flags(&deployer, 0);
        assert!(omni_bridge::pause_flags() == 0, 211);
    }

    #[test(deployer = @omni_bridge)]
    fun nonce_bitmap_set_and_check_range(deployer: signer) {
        let _ = setup(&deployer);
        // Hit boundaries of the 128-bit bitmap word.
        let nonces = vector[0u64, 1, 42, 127, 128, 129, 255, 256, 1000];

        for (i in 0..nonces.length()) {
            assert!(!omni_bridge::is_transfer_finalised(nonces[i]), 220);
        };

        for (i in 0..nonces.length()) {
            omni_bridge::test_mark_nonce_used(nonces[i]);
        };

        for (i in 0..nonces.length()) {
            assert!(omni_bridge::is_transfer_finalised(nonces[i]), 221);
        };

        // Spot-check that gaps remain unset.
        assert!(!omni_bridge::is_transfer_finalised(2), 222);
        assert!(!omni_bridge::is_transfer_finalised(126), 223);
        assert!(!omni_bridge::is_transfer_finalised(130), 224);
    }

    #[test(deployer = @omni_bridge)]
    fun mark_nonce_is_idempotent(deployer: signer) {
        let _ = setup(&deployer);
        omni_bridge::test_mark_nonce_used(42);
        omni_bridge::test_mark_nonce_used(42);
        assert!(omni_bridge::is_transfer_finalised(42), 230);
    }

    #[test(deployer = @omni_bridge)]
    fun nonce_slot_and_bit_packing(deployer: signer) {
        let _ = setup(&deployer);
        let (slot, bit) = omni_bridge::test_nonce_slot_and_bit(0);
        assert!(slot == 0, 240);
        assert!(bit == 1u128, 241);

        let (slot, bit) = omni_bridge::test_nonce_slot_and_bit(127);
        assert!(slot == 0, 242);
        assert!(bit == (1u128 << 127), 243);

        let (slot, bit) = omni_bridge::test_nonce_slot_and_bit(128);
        assert!(slot == 1, 244);
        assert!(bit == 1u128, 245);

        let (slot, bit) = omni_bridge::test_nonce_slot_and_bit(129);
        assert!(slot == 1, 246);
        assert!(bit == 2u128, 247);
    }

    #[test]
    fun normalize_decimals_caps_at_8() {
        assert!(utils::test_normalize_decimals(6) == 6, 250);
        assert!(utils::test_normalize_decimals(8) == 8, 251);
        assert!(utils::test_normalize_decimals(18) == 8, 252);
        assert!(utils::test_normalize_decimals(24) == 8, 253);
    }

    // Recovering a public key from a non-signature byte blob will yield
    // *some* eth address — just not one that matches the configured
    // `near_bridge_derived_address`. The verifier must abort.
    // E_INVALID_SIGNATURE = 3 in omni_bridge::utils.
    #[test]
    #[expected_failure(abort_code = 3, location = omni_bridge::utils)]
    fun verify_eth_signature_rejects_invalid_sig() {
        let sig_rs = vector[];
        for (_i in 0..64) {
            sig_rs.push_back(0xAAu8);
        };
        let expected = vector[];
        for (_i in 0..20) {
            expected.push_back(0xBBu8);
        };
        utils::test_verify_eth_signature(b"hello world", sig_rs, 27, expected);
    }

    // Signature byte length must be exactly 64.
    // E_INVALID_SIGNATURE_LENGTH = 1 in omni_bridge::utils.
    #[test]
    #[expected_failure(abort_code = 1, location = omni_bridge::utils)]
    fun verify_eth_signature_rejects_wrong_length() {
        let sig_rs = vector[];
        for (_i in 0..32) {
            sig_rs.push_back(0u8);
        }; // only 32 bytes
        let expected = vector[];
        for (_i in 0..20) {
            expected.push_back(0u8);
        };
        utils::test_verify_eth_signature(b"x", sig_rs, 27, expected);
    }

    #[test(deployer = @omni_bridge)]
    fun admin_can_update_token_metadata(deployer: signer) {
        let _ = setup(&deployer);
        // Production deploy_token would mint via the bridge object signer;
        // for the test we create directly under the deployer.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"meta_token",
                string::utf8(b"Meta Token"),
                string::utf8(b"MTOK"),
                8
            );
        let token_addr = metadata.object_address();

        // Admin updates both URIs.
        omni_bridge::set_token_metadata(
            &deployer,
            token_addr,
            option::some(
                string::utf8(b"https://example.com/icon.png")
            ),
            option::some(string::utf8(b"https://example.com"))
        );
        assert!(
            fungible_asset::icon_uri(metadata)
                == string::utf8(b"https://example.com/icon.png"),
            300
        );
        assert!(
            fungible_asset::project_uri(metadata)
                == string::utf8(b"https://example.com"),
            301
        );

        // Admin updates only icon_uri; project_uri unchanged.
        omni_bridge::set_token_metadata(
            &deployer,
            token_addr,
            option::some(
                string::utf8(b"https://example.com/icon2.png")
            ),
            option::none()
        );
        assert!(
            fungible_asset::icon_uri(metadata)
                == string::utf8(b"https://example.com/icon2.png"),
            302
        );
        assert!(
            fungible_asset::project_uri(metadata)
                == string::utf8(b"https://example.com"),
            303
        );
    }

    #[test(deployer = @omni_bridge, attacker = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun non_metadata_admin_cannot_update_token_metadata(
        deployer: signer, attacker: signer
    ) {
        let _ = setup(&deployer);
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"meta_token2",
                string::utf8(b"Meta Token 2"),
                string::utf8(b"MTOK2"),
                8
            );
        account::create_account_for_test(attacker.address_of());
        omni_bridge::set_token_metadata(
            &attacker,
            metadata.object_address(),
            option::some(
                string::utf8(b"https://attacker.example/icon.png")
            ),
            option::none()
        );
    }

    #[test(deployer = @omni_bridge, meta = @0xBEEF)]
    fun admin_can_grant_and_revoke_metadata_admin(
        deployer: signer, meta: signer
    ) {
        let _ = setup(&deployer);
        let role = role_id(b"MetadataAdmin");
        assert!(omni_bridge::has_role(role, deployer.address_of()), 310);
        assert!(omni_bridge::role_holders(role).length() == 1, 311);

        // Grant adds, doesn't replace.
        account::create_account_for_test(meta.address_of());
        omni_bridge::grant_role(&deployer, role, meta.address_of());
        assert!(omni_bridge::has_role(role, meta.address_of()), 312);
        assert!(omni_bridge::has_role(role, deployer.address_of()), 313);
        assert!(omni_bridge::role_holders(role).length() == 2, 314);

        // Both holders can call set_token_metadata.
        let m1 =
            bridge_token::test_create(
                &deployer,
                b"meta_g1",
                string::utf8(b"Meta G1"),
                string::utf8(b"MG1"),
                8
            );
        omni_bridge::set_token_metadata(
            &meta,
            m1.object_address(),
            option::some(string::utf8(b"https://m.example/icon.png")),
            option::none()
        );
        let m2 =
            bridge_token::test_create(
                &deployer,
                b"meta_g2",
                string::utf8(b"Meta G2"),
                string::utf8(b"MG2"),
                8
            );
        omni_bridge::set_token_metadata(
            &deployer,
            m2.object_address(),
            option::some(string::utf8(b"https://d.example/icon.png")),
            option::none()
        );

        // Revoke deployer, leaving only `meta`.
        omni_bridge::revoke_role(&deployer, role, deployer.address_of());
        assert!(!omni_bridge::has_role(role, deployer.address_of()), 315);
        assert!(omni_bridge::has_role(role, meta.address_of()), 316);
        assert!(omni_bridge::role_holders(role).length() == 1, 317);
    }

    #[test(deployer = @omni_bridge, meta = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun revoked_metadata_admin_loses_access(
        deployer: signer, meta: signer
    ) {
        let _ = setup(&deployer);
        let role = role_id(b"MetadataAdmin");
        account::create_account_for_test(meta.address_of());
        omni_bridge::grant_role(&deployer, role, meta.address_of());
        omni_bridge::revoke_role(&deployer, role, deployer.address_of());

        // Deployer was the initial metadata_admin but was revoked.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"meta_revoked",
                string::utf8(b"Meta R"),
                string::utf8(b"MR"),
                8
            );
        omni_bridge::set_token_metadata(
            &deployer,
            metadata.object_address(),
            option::some(
                string::utf8(b"https://stale.example/icon.png")
            ),
            option::none()
        );
    }

    #[test(deployer = @omni_bridge, attacker = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun non_admin_cannot_grant_role(deployer: signer, attacker: signer) {
        let _ = setup(&deployer);
        account::create_account_for_test(attacker.address_of());
        omni_bridge::grant_role(
            &attacker, role_id(b"MetadataAdmin"), attacker.address_of()
        );
    }

    #[test(deployer = @omni_bridge, attacker = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun non_admin_cannot_revoke_role(deployer: signer, attacker: signer) {
        let _ = setup(&deployer);
        account::create_account_for_test(attacker.address_of());
        omni_bridge::revoke_role(
            &attacker, role_id(b"MetadataAdmin"), deployer.address_of()
        );
    }

    #[test(deployer = @omni_bridge)]
    #[expected_failure(abort_code = 12, location = omni_bridge::omni_bridge)]
    fun cannot_remove_last_admin(deployer: signer) {
        let _ = setup(&deployer);
        // Deployer is the sole initial Admin; removing them would brick
        // the bridge.
        omni_bridge::revoke_role(&deployer, role_id(b"Admin"), deployer.address_of());
    }

    #[test(deployer = @omni_bridge, co_admin = @0xCAFE2)]
    fun admin_can_step_down_when_another_admin_exists(
        deployer: signer, co_admin: signer
    ) {
        let _ = setup(&deployer);
        let admin_role = role_id(b"Admin");
        account::create_account_for_test(co_admin.address_of());
        omni_bridge::grant_role(&deployer, admin_role, co_admin.address_of());

        // Now there are 2 admins — deployer can step down.
        omni_bridge::revoke_role(&deployer, admin_role, deployer.address_of());
        assert!(!omni_bridge::has_role(admin_role, deployer.address_of()), 320);
        assert!(omni_bridge::has_role(admin_role, co_admin.address_of()), 321);
    }

    #[test(deployer = @omni_bridge)]
    fun grant_is_idempotent(deployer: signer) {
        let _ = setup(&deployer);
        let role = role_id(b"Pauser");
        let len_before = omni_bridge::role_holders(role).length();
        // Granting the existing holder again is a no-op.
        omni_bridge::grant_role(&deployer, role, deployer.address_of());
        assert!(omni_bridge::role_holders(role).length() == len_before, 330);
    }

    #[test(deployer = @omni_bridge)]
    #[expected_failure(abort_code = 11, location = omni_bridge::omni_bridge)]
    fun cannot_update_metadata_of_non_bridge_token(deployer: signer) {
        let native_fa = setup(&deployer);
        // `native_fa` is a plain test FA, not bridge-deployed.
        omni_bridge::set_token_metadata(
            &deployer,
            native_fa.object_address(),
            option::some(
                string::utf8(b"https://attacker.example/icon.png")
            ),
            option::none()
        );
    }

    #[test(deployer = @omni_bridge)]
    fun bridge_token_create_mint_burn(deployer: signer) {
        let _ = setup(&deployer);
        // Bridge token creation uses the resource account in production;
        // here we exercise the underlying module directly.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"my_token",
                string::utf8(b"My Token"),
                string::utf8(b"MTK"),
                8
            );
        assert!(bridge_token::is_bridge_token(metadata), 260);

        let recipient = @0xAAAA;
        account::create_account_for_test(recipient);
        bridge_token::test_mint(metadata, recipient, 1_000);
        assert!(primary_fungible_store::balance(recipient, metadata) == 1_000, 261);

        bridge_token::test_burn(metadata, recipient, 400);
        assert!(primary_fungible_store::balance(recipient, metadata) == 600, 262);

        // Decimals plumbed through.
        assert!(fungible_asset::decimals(metadata) == 8, 263);
    }

    #[test]
    fun metadata_borsh_layout() {
        // type byte (1) + token (4+5) + name (4+8) + symbol (4+3) + decimals (1)
        let payload =
            bridge_types::new_metadata_payload(
                string::utf8(b"hello"),
                string::utf8(b"My Token"),
                string::utf8(b"MTK"),
                18
            );
        let bytes = payload.metadata_to_borsh();
        // Total length sanity.
        let expected_len = 1 + 4 + 5 + 4 + 8 + 4 + 3 + 1;
        assert!(bytes.length() == expected_len, 270);
        // Type byte = 1 (Metadata).
        assert!(bytes[0] == 1, 271);
        // Token length prefix.
        assert!(bytes[1] == 0x05, 272);
        // Final byte = decimals.
        assert!(bytes[expected_len - 1] == 18, 273);
    }

    #[test]
    fun transfer_message_borsh_includes_chain_id_twice() {
        // Two chain_id tags are interleaved between fields per OmniAddress
        // encoding — once before token_address, once before recipient.
        let token_addr: address =
            @0x0000000000000000000000000000000000000000000000000000000000000001;
        let recipient: address =
            @0x0000000000000000000000000000000000000000000000000000000000000002;
        let payload =
            bridge_types::new_transfer_message_payload(
                7u64,
                5u8,
                42u64,
                token_addr,
                1000u128,
                recipient,
                option::none<string::String>(),
                option::none<vector<u8>>()
            );
        let bytes = payload.transfer_message_to_borsh(13u8);
        // Expect 1 type + 8 dest nonce + 1 origin chain + 8 origin nonce +
        //        1 chain + 32 token + 16 amount + 1 chain + 32 recipient + 1 option(None)
        let expected_len = 1 + 8 + 1 + 8 + 1 + 32 + 16 + 1 + 32 + 1;
        assert!(bytes.length() == expected_len, 280);
        assert!(bytes[0] == 0, 281); // TransferMessage tag = 0
        // Origin chain at offset 1 + 8 = 9
        assert!(bytes[9] == 5u8, 282);
        // Chain id tag (first) at offset 1 + 8 + 1 + 8 = 18
        assert!(bytes[18] == 13u8, 283);
        // Chain id tag (second) at offset 18 + 1 + 32 + 16 = 67
        assert!(bytes[67] == 13u8, 284);
        // Final byte is the None tag for fee_recipient.
        assert!(bytes[expected_len - 1] == 0u8, 285);
    }

    #[test]
    fun transfer_message_borsh_with_fee_recipient_and_message() {
        let token_addr: address = @0x01;
        let recipient: address = @0x02;
        let fr = string::utf8(b"near:alice.near");
        let msg = b"hello";
        let payload =
            bridge_types::new_transfer_message_payload(
                1u64,
                0u8,
                1u64,
                token_addr,
                100u128,
                recipient,
                option::some(fr),
                option::some(msg)
            );
        let bytes = payload.transfer_message_to_borsh(13u8);
        // The message field, per Starknet/EVM semantics, is not wrapped with
        // an Option tag — Some(bytes) contributes only the length-prefixed bytes.
        // Layout tail: ...recipient || 1 (Some fee_recipient) || (4 + 15 bytes) || (4 + 5 bytes)
        let head_len = 1 + 8 + 1 + 8 + 1 + 32 + 16 + 1 + 32;
        let fr_len = 1 + 4 + 15;
        let msg_len = 4 + 5;
        assert!(bytes.length() == head_len + fr_len + msg_len, 290);
        // Some-tag (1) for fee_recipient at head_len.
        assert!(bytes[head_len] == 1u8, 291);
    }

    // -------- init_transfer tests --------

    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    fun init_transfer_burns_bridge_token(deployer: signer, user: signer) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        // Create a bridge-deployed FA and mint to the user.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_burn",
                string::utf8(b"IT Burn"),
                string::utf8(b"ITB"),
                8
            );
        bridge_token::test_mint(metadata, user_addr, 1_000);
        assert!(primary_fungible_store::balance(user_addr, metadata) == 1_000, 400);

        let nonce_before = omni_bridge::current_origin_nonce();
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            500u128, // amount
            10u128, // fee
            0u128, // native_fee
            string::utf8(b"near:alice.near"),
            b""
        );

        // Burned, not transferred to bridge.
        assert!(primary_fungible_store::balance(user_addr, metadata) == 500, 401);
        assert!(
            omni_bridge::current_origin_nonce() == nonce_before + 1,
            402
        );
    }

    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    fun init_transfer_locks_non_bridge_token(
        deployer: signer, user: signer
    ) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        // Plain FA, not bridge-deployed → bridge takes the lock path.
        let (metadata, mint_ref) = create_test_fa_with_mint(&deployer, b"it_lock", 8);
        mint_to(&mint_ref, user_addr, 2_000);
        stash_mint_ref(&deployer, b"it_lock_stash", mint_ref);

        let bridge_addr = omni_bridge::bridge_object_address();
        let bridge_before = primary_fungible_store::balance(bridge_addr, metadata);

        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            750u128,
            0u128,
            0u128,
            string::utf8(b"near:bob.near"),
            b""
        );

        // Tokens moved from user → bridge object (not burned).
        assert!(primary_fungible_store::balance(user_addr, metadata) == 1_250, 410);
        assert!(
            primary_fungible_store::balance(bridge_addr, metadata)
                == bridge_before + 750,
            411
        );
    }

    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    fun init_transfer_increments_origin_nonce(
        deployer: signer, user: signer
    ) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_nonce",
                string::utf8(b"IT N"),
                string::utf8(b"ITN"),
                8
            );
        bridge_token::test_mint(metadata, user_addr, 1_000);

        assert!(omni_bridge::current_origin_nonce() == 0, 420);
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            100u128,
            0u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
        assert!(omni_bridge::current_origin_nonce() == 1, 421);
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            100u128,
            0u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
        assert!(omni_bridge::current_origin_nonce() == 2, 422);
    }

    // Pause flag blocks init_transfer.
    // PAUSE_INIT_TRANSFER = 0x01. E_INIT_TRANSFER_PAUSED = 3.
    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    #[expected_failure(abort_code = 3, location = omni_bridge::omni_bridge)]
    fun init_transfer_aborts_when_paused(deployer: signer, user: signer) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_paused",
                string::utf8(b"IT P"),
                string::utf8(b"ITP"),
                8
            );
        bridge_token::test_mint(metadata, user_addr, 1_000);

        omni_bridge::set_pause_flags(&deployer, 0x01);
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            100u128,
            0u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
    }

    // amount == 0 → E_ZERO_AMOUNT = 8.
    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    #[expected_failure(abort_code = 8, location = omni_bridge::omni_bridge)]
    fun init_transfer_rejects_zero_amount(
        deployer: signer, user: signer
    ) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_zero",
                string::utf8(b"IT Z"),
                string::utf8(b"ITZ"),
                8
            );
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            0u128,
            0u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
    }

    // fee >= amount → E_INVALID_FEE = 9.
    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    #[expected_failure(abort_code = 9, location = omni_bridge::omni_bridge)]
    fun init_transfer_rejects_fee_equal_amount(
        deployer: signer, user: signer
    ) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_fee_eq",
                string::utf8(b"IT FE"),
                string::utf8(b"ITFE"),
                8
            );
        bridge_token::test_mint(metadata, user_addr, 1_000);
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            100u128,
            100u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
    }

    // amount > u64::MAX → E_AMOUNT_OVERFLOW = 10.
    #[test(deployer = @omni_bridge, user = @0xA11CE)]
    #[expected_failure(abort_code = 10, location = omni_bridge::omni_bridge)]
    fun init_transfer_rejects_amount_overflow(
        deployer: signer, user: signer
    ) {
        let _ = setup(&deployer);
        let user_addr = user.address_of();
        account::create_account_for_test(user_addr);

        let metadata =
            bridge_token::test_create(
                &deployer,
                b"it_ovf",
                string::utf8(b"IT O"),
                string::utf8(b"ITO"),
                8
            );
        bridge_token::test_mint(metadata, user_addr, 1_000);
        // u128 value > u64::MAX, fee < amount.
        omni_bridge::init_transfer(
            &user,
            metadata.object_address(),
            0x1_0000_0000_0000_0000u128, // u64::MAX + 1
            0u128,
            0u128,
            string::utf8(b"near:x"),
            b""
        );
    }

    // -------- Wormhole payload encoding tests --------
    //
    // Verify each wormhole payload matches the layout that
    // `OmniBridgeWormhole.sol` produces for the equivalent extension. We
    // assert on the leading tag byte + chain-id positions + total length
    // rather than every byte — the underlying primitives (`bcs::to_bytes`,
    // `borsh::encode_string`) are already exhaustively tested in
    // `borsh_tests` and the borsh-layout tests above.

    #[test]
    fun init_transfer_wormhole_payload_layout() {
        let recipient = string::utf8(b"alice.near");
        let message = b"hi";
        let bytes =
            bridge_types::init_transfer_wormhole_payload(
                13u8, // chain_id
                @0xAA,
                @0xBB,
                42u64,
                1_000u128,
                10u128,
                5u128,
                &recipient,
                &message
            );
        // Tag = WH_MSG_INIT_TRANSFER = 0.
        assert!(bytes[0] == 0u8, 500);
        // chain_id at byte 1 and again before token_address (at 1 + 1 + 32 = 34).
        assert!(bytes[1] == 13u8, 501);
        assert!(bytes[34] == 13u8, 502);
        // Layout: tag(1) + cid(1) + sender(32) + cid(1) + token(32)
        //       + origin_nonce(8) + amount(16) + fee(16) + native_fee(16)
        //       + len(4)+"alice.near"(10) + len(4)+"hi"(2)
        // = 1+1+32+1+32+8+16+16+16+4+10+4+2 = 143
        assert!(bytes.length() == 143, 503);
    }

    #[test]
    fun fin_transfer_wormhole_payload_layout_with_fee_recipient() {
        let fr = option::some(string::utf8(b"fee.near"));
        let bytes =
            bridge_types::fin_transfer_wormhole_payload(
                13u8, // chain_id
                7u8, // origin_chain
                42u64,
                @0xCC,
                999u128,
                &fr
            );
        // Tag = WH_MSG_FIN_TRANSFER = 1.
        assert!(bytes[0] == 1u8, 510);
        // origin_chain at byte 1.
        assert!(bytes[1] == 7u8, 511);
        // chain_id at 1 + 1 + 8 = 10 (after tag, origin_chain, origin_nonce).
        assert!(bytes[10] == 13u8, 512);
        // Layout: 1 + 1 + 8 + 1 + 32 + 16 + (4 + 8) = 71
        assert!(bytes.length() == 71, 513);
    }

    #[test]
    fun fin_transfer_wormhole_payload_layout_without_fee_recipient() {
        let fr = option::none<string::String>();
        let bytes =
            bridge_types::fin_transfer_wormhole_payload(
                13u8, 7u8, 42u64, @0xCC, 999u128, &fr
            );
        assert!(bytes[0] == 1u8, 520);
        // Layout: 1 + 1 + 8 + 1 + 32 + 16 + 4 (empty length prefix) = 63
        assert!(bytes.length() == 63, 521);
        // Last 4 bytes are the zero-length prefix.
        let len = bytes.length();
        assert!(bytes[len - 4] == 0u8, 522);
        assert!(bytes[len - 3] == 0u8, 523);
        assert!(bytes[len - 2] == 0u8, 524);
        assert!(bytes[len - 1] == 0u8, 525);
    }

    #[test]
    fun deploy_token_wormhole_payload_layout() {
        let token = string::utf8(b"usdc.near");
        let bytes =
            bridge_types::deploy_token_wormhole_payload(
                13u8,
                &token,
                @0xDD,
                8u8, // decimals
                6u8 // origin_decimals
            );
        // Tag = WH_MSG_DEPLOY_TOKEN = 2.
        assert!(bytes[0] == 2u8, 530);
        // chain_id sits after tag + encoded token (4-byte len + 9 utf-8 bytes).
        let chain_id_offset = 1 + 4 + 9;
        assert!(bytes[chain_id_offset] == 13u8, 531);
        // Layout: 1 + (4 + 9) + 1 + 32 + 1 + 1 = 49
        assert!(bytes.length() == 49, 532);
        // Last two bytes are decimals + origin_decimals.
        let len = bytes.length();
        assert!(bytes[len - 2] == 8u8, 533);
        assert!(bytes[len - 1] == 6u8, 534);
    }

    #[test]
    fun log_metadata_wormhole_payload_layout() {
        let name = string::utf8(b"USDC");
        let symbol = string::utf8(b"USDC");
        let bytes =
            bridge_types::log_metadata_wormhole_payload(
                13u8,
                @0xEE,
                &name,
                &symbol,
                6u8
            );
        // Tag = WH_MSG_LOG_METADATA = 3.
        assert!(bytes[0] == 3u8, 540);
        // chain_id at byte 1.
        assert!(bytes[1] == 13u8, 541);
        // Layout: 1 + 1 + 32 + (4 + 4) + (4 + 4) + 1 = 51
        assert!(bytes.length() == 51, 542);
        // Last byte is decimals.
        let len = bytes.length();
        assert!(bytes[len - 1] == 6u8, 543);
    }

    // -------- set_wormhole_enabled gating --------

    #[test(deployer = @omni_bridge)]
    fun admin_can_toggle_wormhole(deployer: signer) {
        // Round-trip: off → on → off → on. Confirms the bool flips
        // cleanly. The emitter cap is registered once in `initialize` and
        // never re-registered, so all four calls are pure flag flips.
        let _ = setup(&deployer);
        omni_bridge::set_wormhole_enabled(&deployer, true);
        omni_bridge::set_wormhole_enabled(&deployer, false);
        omni_bridge::set_wormhole_enabled(&deployer, true);
    }

    #[test(deployer = @omni_bridge)]
    fun toggling_wormhole_is_idempotent(deployer: signer) {
        // Setting the flag to its current value is a no-op — no abort.
        let _ = setup(&deployer);
        omni_bridge::set_wormhole_enabled(&deployer, false);
        omni_bridge::set_wormhole_enabled(&deployer, false);
        omni_bridge::set_wormhole_enabled(&deployer, true);
        omni_bridge::set_wormhole_enabled(&deployer, true);
    }

    // Non-admin cannot toggle wormhole. E_UNAUTHORIZED = 2.
    #[test(deployer = @omni_bridge, intruder = @0xBADBAD)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun non_admin_cannot_toggle_wormhole(deployer: signer, intruder: signer) {
        let _ = setup(&deployer);
        account::create_account_for_test(intruder.address_of());
        omni_bridge::set_wormhole_enabled(&intruder, true);
    }
}

