pub use OmniBridge::{DeployToken, Event as OmniEvents, LogMetadata};
use starknet::ContractAddress;

#[derive(Drop, Serde)]
pub struct MetadataPayload {
    pub token: ByteArray,
    pub name: ByteArray,
    pub symbol: ByteArray,
    pub decimals: u8,
}

#[starknet::interface]
pub trait IOmniBridge<TContractState> {
    fn log_metadata(ref self: TContractState, token: ContractAddress);
    fn deploy_token(ref self: TContractState, r: u256, s: u256, v: u32, payload: MetadataPayload);
}

#[starknet::contract]
mod OmniBridge {
    use core::keccak::compute_keccak_byte_array;
    use starknet::eth_signature::verify_eth_signature;
    use starknet::event::EventEmitter;
    use starknet::secp256_trait::signature_from_vrs;
    use starknet::storage::{
        Map, StorageMapWriteAccess, StoragePointerReadAccess, StoragePointerWriteAccess,
    };
    use starknet::syscalls::deploy_syscall;
    use starknet::{ClassHash, ContractAddress, EthAddress, SyscallResultTrait, syscalls};
    use crate::utils;
    use crate::utils::{borsh, reverse_u256_bytes};
    use super::MetadataPayload;

    #[derive(Drop, starknet::Event)]
    pub struct LogMetadata {
        #[key]
        pub address: ContractAddress,
        pub name: ByteArray,
        pub symbol: ByteArray,
        pub decimals: u8,
    }

    #[derive(Drop, starknet::Event)]
    pub struct DeployToken {
        #[key]
        pub token_address: ContractAddress,
        pub near_token_id: ByteArray,
        pub name: ByteArray,
        pub symbol: ByteArray,
        pub decimals: u8,
        pub origin_decimals: u8,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        LogMetadata: LogMetadata,
        DeployToken: DeployToken,
    }

    // Used nonces
    // Admin?
    #[storage]
    struct Storage {
        bridge_token_class_hash: ClassHash,
        current_origin_nonce: u64,
        deployed_tokens: Map<ContractAddress, bool>,
        starknet_to_near_token: Map<ContractAddress, ByteArray>,
        // Can't use ByteArray as a key. Using hash instead
        near_to_starknet_token: Map<u256, ContractAddress>,
        omni_bridge_chain_id: u8,
        omni_bridge_derived_address: EthAddress,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        omni_bridge_derived_address: EthAddress,
        omni_bridge_chain_id: u8,
        token_class_hash: ClassHash,
    ) {
        self.omni_bridge_derived_address.write(omni_bridge_derived_address);
        self.omni_bridge_chain_id.write(omni_bridge_chain_id);
        self.bridge_token_class_hash.write(token_class_hash);
    }

    #[abi(embed_v0)]
    #[feature("safe_dispatcher")]
    impl OmniBridgeImpl of super::IOmniBridge<ContractState> {
        fn log_metadata(ref self: ContractState, token: ContractAddress) {
            // There are two possible metadata standards in use.
            // 1. Old style: name and symbol are felt252 values.
            // 2. New style: name and symbol are ByteArray values (ERC20 ABI).
            // We are using low-level contract calls to determine the type.

            let call_data: Array<felt252> = array![];
            let mut res = syscalls::call_contract_syscall(
                token, selector!("name"), call_data.span(),
            )
                .unwrap_syscall();

            let name = if res.len() == 1 {
                // Old standard (felt252)
                let name = OptionTrait::expect(
                    Serde::<felt252>::deserialize(ref res), 'Could not deserialize name',
                );
                utils::felt252_to_string(name)
            } else {
                // New standard (ByteArray)
                OptionTrait::expect(
                    Serde::<ByteArray>::deserialize(ref res), 'Could not deserialize name',
                )
            };

            let mut res = syscalls::call_contract_syscall(
                token, selector!("symbol"), call_data.span(),
            )
                .unwrap_syscall();

            let symbol = if res.len() == 1 {
                // Old standard (felt252)
                let symbol = OptionTrait::expect(
                    Serde::<felt252>::deserialize(ref res), 'Could not deserialize symbol',
                );
                utils::felt252_to_string(symbol)
            } else {
                // New standard (ByteArray)
                OptionTrait::expect(
                    Serde::<ByteArray>::deserialize(ref res), 'Could not deserialize symbol',
                )
            };

            let decimals = {
                let mut res = syscalls::call_contract_syscall(
                    token, selector!("decimals"), call_data.span(),
                )
                    .unwrap_syscall();

                let decimals = OptionTrait::expect(
                    Serde::<u8>::deserialize(ref res), 'Could not deserialize decimals',
                );
                decimals
            };

            self.emit(Event::LogMetadata(LogMetadata { address: token, name, symbol, decimals }))
        }

        fn deploy_token(
            ref self: ContractState, r: u256, s: u256, v: u32, payload: MetadataPayload,
        ) {
            let mut borsh_bytes: ByteArray = "";
            borsh_bytes.append_byte(1); // Payload type
            borsh_bytes.append(@borsh::encode_byte_array(@payload.token));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.name));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.symbol));
            borsh_bytes.append_byte(payload.decimals);

            let message_hash_le = compute_keccak_byte_array(@borsh_bytes);
            let message_hash = reverse_u256_bytes(message_hash_le);

            let signagure = signature_from_vrs(v, r, s);
            verify_eth_signature(message_hash, signagure, self.omni_bridge_derived_address.read());

            let decimals = _normalizeDecimals(payload.decimals);

            let mut constructor_calldata: Array<felt252> = array![];
            (payload.name.clone(), payload.symbol.clone(), decimals)
                .serialize(ref constructor_calldata);

            let (contract_address, _) = deploy_syscall(
                self.bridge_token_class_hash.read(), 0, constructor_calldata.span(), false,
            )
                .unwrap_syscall();

            self.deployed_tokens.write(contract_address, true);
            self.starknet_to_near_token.write(contract_address, payload.token.clone());

            // Keccak would be quite expensive, but let's not optimize prematurely
            let token_id_hash = compute_keccak_byte_array(@payload.token);
            self.near_to_starknet_token.write(token_id_hash, contract_address);

            self
                .emit(
                    Event::DeployToken(
                        DeployToken {
                            token_address: contract_address,
                            near_token_id: payload.token,
                            name: payload.name,
                            symbol: payload.symbol,
                            decimals,
                            origin_decimals: payload.decimals,
                        },
                    ),
                )
        }
    }

    fn _normalizeDecimals(decimals: u8) -> u8 {
        let maxAllowedDecimals: u8 = 18;
        if (decimals > maxAllowedDecimals) {
            return maxAllowedDecimals;
        }
        return decimals;
    }
}
