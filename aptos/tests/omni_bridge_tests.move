#[test_only]
module omni_bridge::omni_bridge_tests {
    use std::option;
    use std::string;
    use aptos_framework::account;
    use aptos_framework::fungible_asset::{Self, Metadata};
    use aptos_framework::object::{Self, Object};
    use aptos_framework::primary_fungible_store;

    use omni_bridge::bridge_token;
    use omni_bridge::bridge_types;
    use omni_bridge::omni_bridge;
    use omni_bridge::utils;

    /// Create a stand-alone FA (not bridge-deployed) usable as `native_token_metadata`
    /// or as the "locked token" path in tests.
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
        object::object_from_constructor_ref<Metadata>(&constructor_ref)
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
    fun normalize_decimals_caps_at_18() {
        assert!(utils::test_normalize_decimals(6) == 6, 250);
        assert!(utils::test_normalize_decimals(18) == 18, 251);
        assert!(utils::test_normalize_decimals(24) == 18, 252);
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
        let token_addr = object::object_address(&metadata);

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
            object::object_address(&metadata),
            option::some(
                string::utf8(b"https://attacker.example/icon.png")
            ),
            option::none()
        );
    }

    #[test(deployer = @omni_bridge, meta = @0xBEEF)]
    fun admin_can_rotate_metadata_admin(deployer: signer, meta: signer) {
        let _ = setup(&deployer);
        assert!(
            omni_bridge::role_holder(role_id(b"MetadataAdmin"))
                == deployer.address_of(),
            310
        );

        account::create_account_for_test(meta.address_of());
        omni_bridge::set_role(&deployer, role_id(b"MetadataAdmin"), meta.address_of());
        assert!(
            omni_bridge::role_holder(role_id(b"MetadataAdmin")) == meta.address_of(),
            311
        );

        // After rotation the new metadata_admin can update; the deployer no longer can.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"meta_token3",
                string::utf8(b"Meta Token 3"),
                string::utf8(b"MTOK3"),
                8
            );
        omni_bridge::set_token_metadata(
            &meta,
            object::object_address(&metadata),
            option::some(
                string::utf8(b"https://rotated.example/icon.png")
            ),
            option::none()
        );
        assert!(
            fungible_asset::icon_uri(metadata)
                == string::utf8(b"https://rotated.example/icon.png"),
            312
        );
    }

    #[test(deployer = @omni_bridge, meta = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun previous_metadata_admin_loses_access_after_rotation(
        deployer: signer, meta: signer
    ) {
        let _ = setup(&deployer);
        account::create_account_for_test(meta.address_of());
        omni_bridge::set_role(&deployer, role_id(b"MetadataAdmin"), meta.address_of());

        // The deployer was the initial metadata_admin but is no longer.
        let metadata =
            bridge_token::test_create(
                &deployer,
                b"meta_token4",
                string::utf8(b"Meta Token 4"),
                string::utf8(b"MTOK4"),
                8
            );
        omni_bridge::set_token_metadata(
            &deployer,
            object::object_address(&metadata),
            option::some(
                string::utf8(b"https://stale.example/icon.png")
            ),
            option::none()
        );
    }

    #[test(deployer = @omni_bridge, attacker = @0xBEEF)]
    #[expected_failure(abort_code = 2, location = omni_bridge::omni_bridge)]
    fun non_admin_cannot_rotate_metadata_admin(
        deployer: signer, attacker: signer
    ) {
        let _ = setup(&deployer);
        account::create_account_for_test(attacker.address_of());
        omni_bridge::set_role(
            &attacker, role_id(b"MetadataAdmin"), attacker.address_of()
        );
    }

    #[test(deployer = @omni_bridge)]
    #[expected_failure(abort_code = 11, location = omni_bridge::omni_bridge)]
    fun cannot_update_metadata_of_non_bridge_token(deployer: signer) {
        let native_fa = setup(&deployer);
        // `native_fa` is a plain test FA, not bridge-deployed.
        omni_bridge::set_token_metadata(
            &deployer,
            object::object_address(&native_fa),
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
}

