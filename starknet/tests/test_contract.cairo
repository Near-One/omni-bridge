use core::keccak::compute_keccak_byte_array;
use omni_bridge::omni_bridge::{
    FinTransfer, IOmniBridgeDispatcher, IOmniBridgeDispatcherTrait, InitTransfer, LogMetadata,
    MetadataPayload, OmniEvents, Signature, TransferMessagePayload,
};
use omni_bridge::utils::{borsh, reverse_u256_bytes};
use openzeppelin::token::erc20::{ERC20ABIDispatcher, ERC20ABIDispatcherTrait};
use openzeppelin::upgrades::interface::{IUpgradeableDispatcher, IUpgradeableDispatcherTrait};
use snforge_std::signature::secp256k1_curve::{Secp256k1CurveKeyPairImpl, Secp256k1CurveSignerImpl};
use snforge_std::{
    ContractClass, ContractClassTrait, DeclareResultTrait, EventSpyAssertionsTrait, declare,
    spy_events, start_cheat_caller_address, stop_cheat_caller_address,
};
use starknet::eth_signature::public_key_point_to_eth_address;
use starknet::{ClassHash, ContractAddress, EthAddress, SyscallResultTrait};

// BridgeToken interface for mint/burn
#[starknet::interface]
trait IBridgeToken<TContractState> {
    fn mint(ref self: TContractState, recipient: ContractAddress, amount: u256);
    fn burn(ref self: TContractState, account: ContractAddress, amount: u256);
}

// Test secret key (arbitrary non-zero u256 < curve order)
const TEST_SECRET_KEY: u256 = 0xDEADBEEF;
// Starknet chain id for the omni bridge
const STARKNET_CHAIN_ID: u8 = 0x10;

fn declare_bridge_token() -> ContractClass {
    let declare_result = declare("BridgeToken").unwrap_syscall();
    *declare_result.contract_class()
}

fn get_test_eth_address() -> felt252 {
    let key_pair = Secp256k1CurveKeyPairImpl::from_secret_key(TEST_SECRET_KEY);
    let eth_addr: EthAddress = public_key_point_to_eth_address(key_pair.public_key);
    let eth_addr_felt: felt252 = eth_addr.into();
    eth_addr_felt
}

fn deploy_bridge_contract() -> (IOmniBridgeDispatcher, ContractAddress) {
    let token_class_hash = declare_bridge_token().class_hash;
    let owner: ContractAddress = 0x123.try_into().unwrap();
    let native_token: ContractAddress =
        0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7
        .try_into()
        .unwrap();

    let derived_address = get_test_eth_address();

    let contract = declare("OmniBridge").unwrap_syscall().contract_class();
    let (contract_address, _) = contract
        .deploy(
            @array![
                derived_address, // omni_bridge_derived_address (from test key pair)
                STARKNET_CHAIN_ID.into(), // omni_bridge_chain_id
                token_class_hash.into(), // bridge_token_class_hash
                owner.into(), // owner
                native_token.into() // native_token_address
            ],
        )
        .unwrap_syscall();
    (IOmniBridgeDispatcher { contract_address }, contract_address)
}

// Sign a message hash using the test key pair (secp256k1 ECDSA)
// Determines the correct v (27 or 28) by verifying against the expected address
fn sign_message(message_hash: u256) -> Signature {
    let key_pair = Secp256k1CurveKeyPairImpl::from_secret_key(TEST_SECRET_KEY);
    let (r, s) = key_pair.sign(message_hash).unwrap();
    let eth_addr: EthAddress = public_key_point_to_eth_address(key_pair.public_key);

    // Try v=27 (y_parity=false), if invalid try v=28 (y_parity=true)
    let sig_27 = starknet::secp256_trait::signature_from_vrs(27, r, s);
    if starknet::eth_signature::is_eth_signature_valid(message_hash, sig_27, eth_addr).is_ok() {
        Signature { r, s, v: 27 }
    } else {
        Signature { r, s, v: 28 }
    }
}

// Build borsh-encoded message for deploy_token (MetadataPayload)
fn build_deploy_token_message(payload: @MetadataPayload) -> u256 {
    let mut borsh_bytes: ByteArray = "";
    borsh_bytes.append_byte(1); // PayloadType::MetadataPayload
    borsh_bytes.append(@borsh::encode_byte_array(payload.token));
    borsh_bytes.append(@borsh::encode_byte_array(payload.name));
    borsh_bytes.append(@borsh::encode_byte_array(payload.symbol));
    borsh_bytes.append_byte(*payload.decimals);

    let hash_le = compute_keccak_byte_array(@borsh_bytes);
    reverse_u256_bytes(hash_le)
}

// Build borsh-encoded message for fin_transfer (TransferMessagePayload)
fn build_fin_transfer_message(payload: @TransferMessagePayload, chain_id: u8) -> u256 {
    let mut borsh_bytes: ByteArray = "";
    borsh_bytes.append_byte(0); // PayloadType::TransferMessage
    borsh_bytes.append(@borsh::encode_u64(*payload.destination_nonce));
    borsh_bytes.append_byte(*payload.origin_chain);
    borsh_bytes.append(@borsh::encode_u64(*payload.origin_nonce));
    borsh_bytes.append_byte(chain_id);
    borsh_bytes.append(@borsh::encode_address(*payload.token_address));
    borsh_bytes.append(@borsh::encode_u128(*payload.amount));
    borsh_bytes.append_byte(chain_id);
    borsh_bytes.append(@borsh::encode_address(*payload.recipient));
    match payload.fee_recipient {
        Option::None => { borsh_bytes.append_byte(0); },
        Option::Some(fee_recipient) => {
            borsh_bytes.append_byte(1);
            borsh_bytes.append(@borsh::encode_byte_array(fee_recipient));
        },
    }
    match payload.message {
        Option::None => {},
        Option::Some(message) => { borsh_bytes.append(@borsh::encode_byte_array(message)); },
    }

    let hash_le = compute_keccak_byte_array(@borsh_bytes);
    reverse_u256_bytes(hash_le)
}

fn sign_deploy_token(payload: @MetadataPayload) -> Signature {
    let message_hash = build_deploy_token_message(payload);
    sign_message(message_hash)
}

fn deploy_test_token(
    dispatcher: IOmniBridgeDispatcher, _bridge_address: ContractAddress,
) -> ContractAddress {
    let payload = MetadataPayload {
        token: "omni-demo.cfi-pre.near", name: "CFI Token", symbol: "CFI", decimals: 18,
    };

    let signature = sign_deploy_token(@payload);

    dispatcher.deploy_token(signature, payload);

    dispatcher.get_token_address("omni-demo.cfi-pre.near")
}

#[test]
fn test_log_metadata_new_standard() {
    let token_contract = declare_bridge_token();
    let (token_address, _) = token_contract
        .deploy(@array![0, 0x455448, // ETH
        3, 0, 0x455448, // ETH
        3, 12 // decimals
        ])
        .unwrap_syscall();

    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    dispatcher.log_metadata(token_address);

    let expected_event = OmniEvents::LogMetadata(
        LogMetadata {
            address: token_address, name: "\x45\x54\x48", symbol: "\x45\x54\x48", decimals: 12,
        },
    );

    spy.assert_emitted(@array![(bridge_address, expected_event)]);
}

#[test]
fn test_log_metadata_old_standard() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    let eth_token_address = 0x49D36570D4E46F48E99674BD3FCC84644DDD6B96F7C741B1562B82F9E004DC7
        .try_into()
        .unwrap();
    dispatcher.log_metadata(eth_token_address);

    let expected_event = OmniEvents::LogMetadata(
        LogMetadata {
            address: eth_token_address, name: "\x45\x54\x48", symbol: "\x45\x54\x48", decimals: 18,
        },
    );

    spy.assert_emitted(@array![(bridge_address, expected_event)]);
}

#[test]
fn test_deploy_token() {
    let (dispatcher, _bridge_address) = deploy_bridge_contract();

    let payload = MetadataPayload {
        token: "omni-demo.cfi-pre.near", name: "CFI Token", symbol: "CFI", decimals: 18,
    };
    let signature = sign_deploy_token(@payload);

    // Verify deploy_token succeeds with valid signature
    dispatcher.deploy_token(signature, payload);
}

#[test]
fn test_init_transfer_with_bridge_token() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy a bridge token
    let token_address = deploy_test_token(dispatcher, bridge_address);

    // Mint tokens to a user by impersonating the bridge
    let user: ContractAddress = 0x123.try_into().unwrap();
    start_cheat_caller_address(token_address, bridge_address);
    IBridgeTokenDispatcher { contract_address: token_address }.mint(user, 1000);
    stop_cheat_caller_address(token_address);

    // Call init_transfer as the user
    start_cheat_caller_address(bridge_address, user);
    dispatcher.init_transfer(token_address, 800, 50, 0, "recipient.near", "test message");
    stop_cheat_caller_address(bridge_address);

    // Verify tokens were burned (balance should be 200)
    let balance = ERC20ABIDispatcher { contract_address: token_address }.balance_of(user);
    assert_eq!(balance, 200);

    // Verify event
    let expected_event = OmniEvents::InitTransfer(
        InitTransfer {
            sender: user,
            token_address,
            origin_nonce: 1,
            amount: 800,
            fee: 50,
            native_fee: 0,
            recipient: "recipient.near",
            message: "test message",
        },
    );
    spy.assert_emitted(@array![(bridge_address, expected_event)]);
}

#[test]
#[should_panic(expected: ('ERR_INVALID_FEE',))]
fn test_init_transfer_fee_exceeds_amount() {
    let (dispatcher, _) = deploy_bridge_contract();
    let token_address: ContractAddress = 0x456.try_into().unwrap();

    // fee >= amount should fail
    dispatcher.init_transfer(token_address, 100, 100, 0, "recipient.near", "");
}

#[test]
fn test_init_transfer_nonce_increments() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy and setup token
    let token_address = deploy_test_token(dispatcher, bridge_address);

    let user: ContractAddress = 0x123.try_into().unwrap();
    start_cheat_caller_address(token_address, bridge_address);
    IBridgeTokenDispatcher { contract_address: token_address }.mint(user, 10000);
    stop_cheat_caller_address(token_address);

    // Call init_transfer multiple times
    start_cheat_caller_address(bridge_address, user);
    dispatcher.init_transfer(token_address, 100, 10, 0, "recipient1.near", "");
    dispatcher.init_transfer(token_address, 200, 20, 0, "recipient2.near", "");
    dispatcher.init_transfer(token_address, 300, 30, 0, "recipient3.near", "");
    stop_cheat_caller_address(bridge_address);

    // Verify nonces increment (1, 2, 3)
    let expected_event1 = OmniEvents::InitTransfer(
        InitTransfer {
            sender: user,
            token_address,
            origin_nonce: 1,
            amount: 100,
            fee: 10,
            native_fee: 0,
            recipient: "recipient1.near",
            message: "",
        },
    );
    let expected_event2 = OmniEvents::InitTransfer(
        InitTransfer {
            sender: user,
            token_address,
            origin_nonce: 2,
            amount: 200,
            fee: 20,
            native_fee: 0,
            recipient: "recipient2.near",
            message: "",
        },
    );
    let expected_event3 = OmniEvents::InitTransfer(
        InitTransfer {
            sender: user,
            token_address,
            origin_nonce: 3,
            amount: 300,
            fee: 30,
            native_fee: 0,
            recipient: "recipient3.near",
            message: "",
        },
    );

    spy
        .assert_emitted(
            @array![
                (bridge_address, expected_event1), (bridge_address, expected_event2),
                (bridge_address, expected_event3),
            ],
        );
}

#[test]
fn test_fin_transfer_with_bridge_token() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy a bridge token first
    let deploy_payload = MetadataPayload {
        token: "test.token.near", name: "Test Token", symbol: "TT", decimals: 18,
    };
    let deploy_sig = sign_deploy_token(@deploy_payload);
    dispatcher.deploy_token(deploy_sig, deploy_payload);

    // Get the deployed token address
    let token_address = dispatcher.get_token_address("test.token.near");
    let recipient: ContractAddress = 0x999.try_into().unwrap();

    // Build transfer payload
    let transfer_payload = TransferMessagePayload {
        destination_nonce: 1,
        origin_chain: 2,
        origin_nonce: 100,
        token_address,
        amount: 1000,
        recipient,
        fee_recipient: Option::None,
        message: Option::None,
    };

    // Sign the transfer message (borsh encode + keccak + ECDSA)
    let message_hash = build_fin_transfer_message(@transfer_payload, STARKNET_CHAIN_ID);
    let fin_signature = sign_message(message_hash);

    dispatcher.fin_transfer(fin_signature, transfer_payload);

    // Verify tokens were minted
    let balance = ERC20ABIDispatcher { contract_address: token_address }.balance_of(recipient);
    assert_eq!(balance, 1000);

    // Verify event
    let expected_event = OmniEvents::FinTransfer(
        FinTransfer {
            origin_chain: 2,
            origin_nonce: 100,
            token_address,
            amount: 1000,
            recipient,
            fee_recipient: Option::None,
            message: Option::None,
        },
    );
    spy.assert_emitted(@array![(bridge_address, expected_event)]);
}

#[test]
fn test_omni_bridge_upgrade() {
    let (_bridge, bridge_address) = deploy_bridge_contract();
    let owner: ContractAddress = 0x123.try_into().unwrap();

    // Use a real declared class hash - the bridge token class
    let new_class_hash: ClassHash = declare_bridge_token().class_hash;
    start_cheat_caller_address(bridge_address, owner);

    let upgradeable = IUpgradeableDispatcher { contract_address: bridge_address };
    upgradeable.upgrade(new_class_hash);

    stop_cheat_caller_address(bridge_address);
}

#[test]
#[should_panic(expected: ('Caller is missing role',))]
fn test_omni_bridge_upgrade_non_owner_fails() {
    let (_bridge, bridge_address) = deploy_bridge_contract();
    let non_owner: ContractAddress = 0x456.try_into().unwrap();

    start_cheat_caller_address(bridge_address, non_owner);

    let upgradeable = IUpgradeableDispatcher { contract_address: bridge_address };
    let new_class_hash: ClassHash = declare_bridge_token().class_hash;
    upgradeable.upgrade(new_class_hash);

    stop_cheat_caller_address(bridge_address);
}

#[test]
fn test_upgrade_deployed_token() {
    let (bridge, bridge_address) = deploy_bridge_contract();
    let token_address = deploy_test_token(bridge, bridge_address);
    let owner: ContractAddress = 0x123.try_into().unwrap();

    // Declare a new token class to upgrade to
    let new_token_class_hash: ClassHash = declare_bridge_token().class_hash;

    // Bridge owner can upgrade deployed tokens
    start_cheat_caller_address(bridge_address, owner);
    bridge.upgrade_token(token_address, new_token_class_hash);
    stop_cheat_caller_address(bridge_address);
}

#[test]
#[should_panic(expected: ('Caller is missing role',))]
fn test_upgrade_token_non_owner_fails() {
    let (bridge, bridge_address) = deploy_bridge_contract();
    let token_address = deploy_test_token(bridge, bridge_address);
    let non_owner: ContractAddress = 0x456.try_into().unwrap();

    let new_token_class_hash: ClassHash = declare_bridge_token().class_hash;

    // Non-owner cannot upgrade tokens
    start_cheat_caller_address(bridge_address, non_owner);
    bridge.upgrade_token(token_address, new_token_class_hash);
    stop_cheat_caller_address(bridge_address);
}

#[test]
#[should_panic(expected: ('ERR_NOT_BRIDGE_TOKEN',))]
fn test_upgrade_token_not_deployed_by_bridge_fails() {
    let (bridge, bridge_address) = deploy_bridge_contract();
    let owner: ContractAddress = 0x123.try_into().unwrap();

    // Create a random token address that wasn't deployed by the bridge
    let random_token: ContractAddress = 0x999.try_into().unwrap();
    let new_token_class_hash: ClassHash = declare_bridge_token().class_hash;

    // Should fail because token wasn't deployed by bridge
    start_cheat_caller_address(bridge_address, owner);
    bridge.upgrade_token(random_token, new_token_class_hash);
    stop_cheat_caller_address(bridge_address);
}
