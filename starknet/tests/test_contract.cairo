use omni_bridge::omni_bridge::{
    DeployToken, IOmniBridgeDispatcher, IOmniBridgeDispatcherTrait, LogMetadata, MetadataPayload,
    OmniEvents,
};
use openzeppelin::token::erc20::{ERC20ABIDispatcher, ERC20ABIDispatcherTrait};
use snforge_std::{
    ContractClass, ContractClassTrait, DeclareResultTrait, EventSpyAssertionsTrait, declare,
    spy_events,
};
use starknet::{ContractAddress, SyscallResultTrait};

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
    let (dispatcher, bridge_address) = deploy_bridge_contract();
    let mut spy = spy_events();

    let payload = MetadataPayload {
        token: "omni-demo.cfi-pre.near", name: "CFI Token", symbol: "CFI", decimals: 18,
    };

    let v = 28;
    let r = 0xD4E6B9E5FBB3750D6C738D84EDECC0559415914591AB506AA801A0843251FE0B;
    let s = 0x07D29F740974576B2AA90BA836A6B9CA8E7CAB3B8894D42669317CE78822CCB5;
    dispatcher.deploy_token(r, s, v, payload);

    // Address is deterministic so we can use it to simplify the assertion.
    let token_address: ContractAddress =
        2626693339582466100930242932923001456720103221254075542339713793819993762631
        .try_into()
        .unwrap();
    let expected_event = OmniEvents::DeployToken(
        DeployToken {
            token_address,
            near_token_id: "omni-demo.cfi-pre.near",
            name: "CFI Token",
            symbol: "CFI",
            decimals: 18,
            origin_decimals: 18,
        },
    );

    spy.assert_emitted(@array![(bridge_address, expected_event)]);

    let metadata_dispatcher = ERC20ABIDispatcher { contract_address: token_address };
    assert_eq!(metadata_dispatcher.name(), "CFI Token");
    assert_eq!(metadata_dispatcher.symbol(), "CFI");
    assert_eq!(metadata_dispatcher.decimals(), 18);
}
