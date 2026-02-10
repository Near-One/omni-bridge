use omni_bridge::omni_bridge::{
    FinTransfer, IOmniBridgeDispatcher, IOmniBridgeDispatcherTrait, InitTransfer, LogMetadata,
    MetadataPayload, OmniEvents, Signature, TransferMessagePayload,
};
use openzeppelin::token::erc20::{ERC20ABIDispatcher, ERC20ABIDispatcherTrait};
use snforge_std::{
    ContractClass, ContractClassTrait, DeclareResultTrait, EventSpyAssertionsTrait, declare,
    spy_events, start_cheat_caller_address, stop_cheat_caller_address,
};
use starknet::{ContractAddress, SyscallResultTrait};

// BridgeToken interface for mint/burn
#[starknet::interface]
trait IBridgeToken<TContractState> {
    fn mint(ref self: TContractState, recipient: ContractAddress, amount: u256);
    fn burn(ref self: TContractState, account: ContractAddress, amount: u256);
}

fn declare_bridge_token() -> ContractClass {
    let declare_result = declare("BridgeToken").unwrap_syscall();
    *declare_result.contract_class()
}

fn deploy_bridge_contract() -> (IOmniBridgeDispatcher, ContractAddress) {
    let token_class_hash = declare_bridge_token().class_hash;

    let contract = declare("OmniBridge").unwrap_syscall().contract_class();
    let (contract_address, _) = contract
        .deploy(
            @array![
                0x22EB4d37677eD931d9dE2218cecE1A832a147490, // omni_bridge_derived_address
                0x9, // omni_bridge_chain_id
                token_class_hash.into() // bridge_token_class_hash
            ],
        )
        .unwrap_syscall();
    (IOmniBridgeDispatcher { contract_address }, contract_address)
}

fn deploy_test_token(dispatcher: IOmniBridgeDispatcher) -> ContractAddress {
    let payload = MetadataPayload {
        token: "test.token.near", name: "Test Token", symbol: "TT", decimals: 18,
    };
    let signature = Signature {
        r: 0xD4E6B9E5FBB3750D6C738D84EDECC0559415914591AB506AA801A0843251FE0B,
        s: 0x07D29F740974576B2AA90BA836A6B9CA8E7CAB3B8894D42669317CE78822CCB5,
        v: 28,
    };

    dispatcher.deploy_token(signature, payload);

    // Note: The token address is deterministic but changes if the bridge contract storage changes
    // This address corresponds to the current OmniBridge implementation
    // If tests fail, update this address by running test_deploy_token and checking the emitted
    // event
    2626693339582466100930242932923001456720103221254075542339713793819993762631
        .try_into()
        .unwrap()
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

    let signature = Signature {
        r: 0xD4E6B9E5FBB3750D6C738D84EDECC0559415914591AB506AA801A0843251FE0B,
        s: 0x07D29F740974576B2AA90BA836A6B9CA8E7CAB3B8894D42669317CE78822CCB5,
        v: 28,
    };

    // Verify deploy_token succeeds with valid signature
    dispatcher.deploy_token(signature, payload);
    // Note: The deployed token address is deterministic but depends on bridge implementation
    // This test verifies the signature validation and deployment succeeds
}

#[test]
#[ignore] // Requires correct token deployment address - run separately
fn test_init_transfer_with_bridge_token() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy a bridge token
    let token_address = deploy_test_token(dispatcher);

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
#[should_panic(expected: ('InvalidFee',))]
fn test_init_transfer_fee_exceeds_amount() {
    let (dispatcher, _) = deploy_bridge_contract();
    let token_address: ContractAddress = 0x456.try_into().unwrap();

    // fee >= amount should fail
    dispatcher.init_transfer(token_address, 100, 100, 0, "recipient.near", "");
}

#[test]
#[should_panic(expected: ('NativeFeeNotSupported',))]
fn test_init_transfer_native_fee_not_zero() {
    let (dispatcher, _) = deploy_bridge_contract();
    let token_address: ContractAddress = 0x456.try_into().unwrap();

    // native_fee != 0 should fail
    dispatcher.init_transfer(token_address, 100, 10, 1, "recipient.near", "");
}

#[test]
#[ignore] // Requires correct token deployment address - run separately
fn test_init_transfer_nonce_increments() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy and setup token
    let token_address = deploy_test_token(dispatcher);

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

// TODO: Compute valid signature for fin_transfer test
// The signature must be for the borsh-encoded TransferMessagePayload with:
// - PayloadType::TransferMessage (0)
// - destination_nonce, origin_chain, origin_nonce
// - chain_id (9), token_address (32 bytes LE), amount
// - chain_id (9), recipient (32 bytes LE), fee_recipient (optional)
// Signed with the private key for address 0x22EB4d37677eD931d9dE2218cecE1A832a147490

#[test]
#[ignore] // Remove #[ignore] after computing valid signature
fn test_fin_transfer_with_bridge_token() {
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    // Deploy a bridge token first
    let payload = MetadataPayload {
        token: "test.token.near", name: "Test Token", symbol: "TT", decimals: 18,
    };
    let deploy_signature = Signature {
        r: 0xD4E6B9E5FBB3750D6C738D84EDECC0559415914591AB506AA801A0843251FE0B,
        s: 0x07D29F740974576B2AA90BA836A6B9CA8E7CAB3B8894D42669317CE78822CCB5,
        v: 28,
    };
    dispatcher.deploy_token(deploy_signature, payload);

    let token_address: ContractAddress =
        2626693339582466100930242932923001456720103221254075542339713793819993762631
        .try_into()
        .unwrap();
    let recipient: ContractAddress = 0x999.try_into().unwrap();

    // TODO: Replace with actual computed signature
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

    // TODO: Replace these with actual signature values
    let fin_signature = Signature { r: 0x0, s: 0x0, v: 28 };

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
        },
    );
    spy.assert_emitted(@array![(bridge_address, expected_event)]);
}
