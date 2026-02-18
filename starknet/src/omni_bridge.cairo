pub use OmniBridge::Event as OmniEvents;
use starknet::{ClassHash, ContractAddress};
pub use crate::bridge_types::{
    DeployToken, FinTransfer, InitTransfer, LogMetadata, MetadataPayload, Signature,
    TransferMessagePayload,
};

#[starknet::interface]
pub trait IOmniBridge<TContractState> {
    fn log_metadata(ref self: TContractState, token: ContractAddress);
    fn deploy_token(ref self: TContractState, signature: Signature, payload: MetadataPayload);
    fn fin_transfer(
        ref self: TContractState, signature: Signature, payload: TransferMessagePayload,
    );
    fn init_transfer(
        ref self: TContractState,
        token_address: ContractAddress,
        amount: u128,
        fee: u128,
        native_fee: u128,
        recipient: ByteArray,
        message: ByteArray,
    );
    fn upgrade_token(
        ref self: TContractState, token_address: ContractAddress, new_class_hash: ClassHash,
    );
    fn set_pause_flags(ref self: TContractState, flags: u8);
    fn pause_all(ref self: TContractState);
    fn get_token_address(self: @TContractState, token_id: ByteArray) -> ContractAddress;
}

#[starknet::contract]
mod OmniBridge {
    use core::keccak::compute_keccak_byte_array;
    use core::num::traits::Zero;
    use openzeppelin::access::accesscontrol::AccessControlComponent;
    use openzeppelin::introspection::src5::SRC5Component;
    use openzeppelin::token::erc20::interface::{IERC20Dispatcher, IERC20DispatcherTrait};
    use openzeppelin::upgrades::interface::{
        IUpgradeable, IUpgradeableDispatcher, IUpgradeableDispatcherTrait,
    };
    use openzeppelin::upgrades::upgradeable::UpgradeableComponent;
    use starknet::eth_signature::verify_eth_signature;
    use starknet::event::EventEmitter;
    use starknet::secp256_trait::signature_from_vrs;
    use starknet::storage::{
        Map, StorageMapReadAccess, StorageMapWriteAccess, StoragePointerReadAccess,
        StoragePointerWriteAccess,
    };
    use starknet::syscalls::deploy_syscall;
    use starknet::{
        ClassHash, ContractAddress, EthAddress, SyscallResultTrait, get_caller_address,
        get_contract_address, syscalls,
    };
    use crate::bridge_types::{
        DeployToken, FinTransfer, InitTransfer, LogMetadata, MetadataPayload, PauseStateChanged,
        Signature, TransferMessagePayload,
    };
    use crate::utils;
    use crate::utils::{borsh, reverse_u256_bytes};

    // Role constants
    const DEFAULT_ADMIN_ROLE: felt252 = 0;
    const PAUSER_ROLE: felt252 = selector!("PAUSER_ROLE");

    // Pause flag constants
    const PAUSE_INIT_TRANSFER: u8 = 0x01; // 0001
    const PAUSE_FIN_TRANSFER: u8 = 0x02; // 0010
    const PAUSE_DEPLOY_TOKEN: u8 = 0x04; // 0100
    const PAUSE_ALL: u8 = 0xFF; // 1111

    component!(path: AccessControlComponent, storage: accesscontrol, event: AccessControlEvent);
    component!(path: SRC5Component, storage: src5, event: SRC5Event);
    component!(path: UpgradeableComponent, storage: upgradeable, event: UpgradeableEvent);

    #[abi(embed_v0)]
    impl AccessControlMixinImpl =
        AccessControlComponent::AccessControlMixinImpl<ContractState>;
    impl AccessControlInternalImpl = AccessControlComponent::InternalImpl<ContractState>;
    impl UpgradeableInternalImpl = UpgradeableComponent::InternalImpl<ContractState>;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        LogMetadata: LogMetadata,
        DeployToken: DeployToken,
        InitTransfer: InitTransfer,
        FinTransfer: FinTransfer,
        PauseStateChanged: PauseStateChanged,
        #[flat]
        AccessControlEvent: AccessControlComponent::Event,
        #[flat]
        SRC5Event: SRC5Component::Event,
        #[flat]
        UpgradeableEvent: UpgradeableComponent::Event,
    }

    // Used nonces
    #[storage]
    struct Storage {
        #[substorage(v0)]
        accesscontrol: AccessControlComponent::Storage,
        #[substorage(v0)]
        src5: SRC5Component::Storage,
        #[substorage(v0)]
        upgradeable: UpgradeableComponent::Storage,
        pause_flags: u8,
        bridge_token_class_hash: ClassHash,
        current_origin_nonce: u64,
        // Bitmap: slot = nonce / 251, bit = nonce % 251
        completed_transfers: Map<u64, felt252>,
        deployed_tokens: Map<ContractAddress, bool>,
        starknet_to_near_token: Map<ContractAddress, ByteArray>,
        // Can't use ByteArray as a key. Using hash instead
        near_to_starknet_token: Map<u256, ContractAddress>,
        omni_bridge_chain_id: u8,
        omni_bridge_derived_address: EthAddress,
        native_token_address: ContractAddress,
    }

    #[constructor]
    fn constructor(
        ref self: ContractState,
        omni_bridge_derived_address: EthAddress,
        omni_bridge_chain_id: u8,
        token_class_hash: ClassHash,
        default_admin: ContractAddress,
        native_token_address: ContractAddress,
    ) {
        self.omni_bridge_derived_address.write(omni_bridge_derived_address);
        self.omni_bridge_chain_id.write(omni_bridge_chain_id);
        self.bridge_token_class_hash.write(token_class_hash);
        self.native_token_address.write(native_token_address);
        self.pause_flags.write(0); // Not paused initially

        // Initialize AccessControl with admin role
        self.accesscontrol.initializer();
        self.accesscontrol._grant_role(DEFAULT_ADMIN_ROLE, default_admin);
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

        fn deploy_token(ref self: ContractState, signature: Signature, payload: MetadataPayload) {
            // Check if deploy_token is paused
            assert(!_is_paused(@self, PAUSE_DEPLOY_TOKEN), 'ERR_DEPLOY_TOKEN_PAUSED');

            let mut borsh_bytes: ByteArray = "";
            borsh_bytes.append_byte(1); // Payload type
            borsh_bytes.append(@borsh::encode_byte_array(@payload.token));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.name));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.symbol));
            borsh_bytes.append_byte(payload.decimals);

            _verify_borsh_signature(ref self, @borsh_bytes, signature);

            // Verify token hasn't been deployed yet
            let token_id_hash = compute_keccak_byte_array(@payload.token);
            let existing_token = self.near_to_starknet_token.read(token_id_hash);
            assert(existing_token.is_zero(), 'ERR_TOKEN_ALREADY_DEPLOYED');

            let decimals = _normalizeDecimals(payload.decimals);

            let mut constructor_calldata: Array<felt252> = array![];
            (payload.name.clone(), payload.symbol.clone(), decimals)
                .serialize(ref constructor_calldata);

            // Use token_id_hash as salt for deterministic deployment
            // Use the low part of the u256 hash to ensure it fits in felt252
            let salt: felt252 = token_id_hash.low.into();
            let (contract_address, _) = deploy_syscall(
                self.bridge_token_class_hash.read(), salt, constructor_calldata.span(), false,
            )
                .unwrap_syscall();

            self.deployed_tokens.write(contract_address, true);
            self.starknet_to_near_token.write(contract_address, payload.token.clone());
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

        fn fin_transfer(
            ref self: ContractState, signature: Signature, payload: TransferMessagePayload,
        ) {
            // Check if fin_transfer is paused
            assert(!_is_paused(@self, PAUSE_FIN_TRANSFER), 'ERR_FIN_TRANSFER_PAUSED');

            assert(
                !_is_transfer_finalised(@self, payload.destination_nonce), 'ERR_NONCE_ALREADY_USED',
            );
            _set_transfer_finalised(ref self, payload.destination_nonce);

            let chain_id = self.omni_bridge_chain_id.read();

            let mut borsh_bytes: ByteArray = "";
            borsh_bytes.append_byte(0); // PayloadType::TransferMessage
            borsh_bytes.append(@borsh::encode_u64(payload.destination_nonce));
            borsh_bytes.append_byte(payload.origin_chain);
            borsh_bytes.append(@borsh::encode_u64(payload.origin_nonce));
            borsh_bytes.append_byte(chain_id);
            borsh_bytes.append(@borsh::encode_address(payload.token_address));
            borsh_bytes.append(@borsh::encode_u128(payload.amount));
            borsh_bytes.append_byte(chain_id);
            borsh_bytes.append(@borsh::encode_address(payload.recipient));
            match @payload.fee_recipient {
                Option::None => { borsh_bytes.append_byte(0); },
                Option::Some(fee_recipient) => {
                    borsh_bytes.append_byte(1);
                    borsh_bytes.append(@borsh::encode_byte_array(fee_recipient));
                },
            }
            match @payload.message {
                Option::None => {},
                Option::Some(message) => {
                    borsh_bytes.append(@borsh::encode_byte_array(message));
                },
            }

            _verify_borsh_signature(ref self, @borsh_bytes, signature);

            if self.deployed_tokens.read(payload.token_address) {
                IBridgeTokenDispatcher { contract_address: payload.token_address }
                    .mint(payload.recipient, payload.amount.into());
            } else {
                let success = IERC20Dispatcher { contract_address: payload.token_address }
                    .transfer(payload.recipient, payload.amount.into());
                assert(success, 'ERR_TRANSFER_FAILED');
            }

            self
                .emit(
                    Event::FinTransfer(
                        FinTransfer {
                            origin_chain: payload.origin_chain,
                            origin_nonce: payload.origin_nonce,
                            token_address: payload.token_address,
                            amount: payload.amount,
                            recipient: payload.recipient,
                            fee_recipient: payload.fee_recipient,
                            message: payload.message,
                        },
                    ),
                )
        }

        fn init_transfer(
            ref self: ContractState,
            token_address: ContractAddress,
            amount: u128,
            fee: u128,
            native_fee: u128,
            recipient: ByteArray,
            message: ByteArray,
        ) {
            // Check if init_transfer is paused
            assert(!_is_paused(@self, PAUSE_INIT_TRANSFER), 'ERR_INIT_TRANSFER_PAUSED');

            assert(amount > 0, 'ERR_ZERO_AMOUNT');
            assert(fee < amount, 'ERR_INVALID_FEE');

            self.current_origin_nonce.write(self.current_origin_nonce.read() + 1);

            let caller = get_caller_address();

            // Handle token transfer (burn or lock)
            if self.deployed_tokens.read(token_address) {
                IBridgeTokenDispatcher { contract_address: token_address }
                    .burn(caller, amount.into());
            } else {
                let success = IERC20Dispatcher { contract_address: token_address }
                    .transfer_from(caller, get_contract_address(), amount.into());
                assert(success, 'ERR_TRANSFER_FROM_FAILED');
            }

            // Handle native fee payment if specified
            if native_fee > 0 {
                let native_token = self.native_token_address.read();
                let success = IERC20Dispatcher { contract_address: native_token }
                    .transfer_from(caller, get_contract_address(), native_fee.into());
                assert(success, 'ERR_FEE_TRANSFER_FAILED');
            }

            self
                .emit(
                    Event::InitTransfer(
                        InitTransfer {
                            sender: caller,
                            token_address,
                            origin_nonce: self.current_origin_nonce.read(),
                            amount,
                            fee,
                            native_fee,
                            recipient,
                            message,
                        },
                    ),
                )
        }

        fn upgrade_token(
            ref self: ContractState, token_address: ContractAddress, new_class_hash: ClassHash,
        ) {
            self.accesscontrol.assert_only_role(DEFAULT_ADMIN_ROLE);
            assert(self.deployed_tokens.read(token_address), 'ERR_NOT_BRIDGE_TOKEN');

            let upgradeable = IUpgradeableDispatcher { contract_address: token_address };
            upgradeable.upgrade(new_class_hash);
        }

        fn set_pause_flags(ref self: ContractState, flags: u8) {
            self.accesscontrol.assert_only_role(DEFAULT_ADMIN_ROLE);
            let old_flags = self.pause_flags.read();
            self.pause_flags.write(flags);

            self
                .emit(
                    Event::PauseStateChanged(
                        PauseStateChanged {
                            old_flags, new_flags: flags, admin: get_caller_address(),
                        },
                    ),
                );
        }

        fn pause_all(ref self: ContractState) {
            self.accesscontrol.assert_only_role(PAUSER_ROLE);
            let old_flags = self.pause_flags.read();
            self.pause_flags.write(PAUSE_ALL);

            self
                .emit(
                    Event::PauseStateChanged(
                        PauseStateChanged {
                            old_flags, new_flags: PAUSE_ALL, admin: get_caller_address(),
                        },
                    ),
                );
        }

        fn get_token_address(self: @ContractState, token_id: ByteArray) -> ContractAddress {
            let token_id_hash = compute_keccak_byte_array(@token_id);
            self.near_to_starknet_token.read(token_id_hash)
        }
    }

    #[abi(embed_v0)]
    impl UpgradeableImpl of IUpgradeable<ContractState> {
        fn upgrade(ref self: ContractState, new_class_hash: ClassHash) {
            self.accesscontrol.assert_only_role(DEFAULT_ADMIN_ROLE);
            self.upgradeable.upgrade(new_class_hash);
        }
    }

    // Helper functions
    fn _verify_borsh_signature(
        ref self: ContractState, borsh_bytes: @ByteArray, signature: Signature,
    ) {
        let message_hash_le = compute_keccak_byte_array(borsh_bytes);
        let message_hash = reverse_u256_bytes(message_hash_le);

        let sig = signature_from_vrs(signature.v, signature.r, signature.s);
        verify_eth_signature(message_hash, sig, self.omni_bridge_derived_address.read());
    }

    fn _is_paused(self: @ContractState, flag: u8) -> bool {
        let flags = self.pause_flags.read();
        (flags & flag) != 0
    }

    #[starknet::interface]
    trait IBridgeToken<TContractState> {
        fn mint(ref self: TContractState, recipient: ContractAddress, amount: u256);
        fn burn(ref self: TContractState, account: ContractAddress, amount: u256);
    }

    fn _nonce_slot_and_bit(nonce: u64) -> (u64, u256) {
        let slot = nonce / 251;
        let bit: u256 = _pow2_felt((nonce % 251).into());
        (slot, bit)
    }

    fn _is_transfer_finalised(self: @ContractState, nonce: u64) -> bool {
        let (slot, bit) = _nonce_slot_and_bit(nonce);
        let bitmap: u256 = self.completed_transfers.read(slot).into();
        bitmap & bit != 0
    }

    fn _set_transfer_finalised(ref self: ContractState, nonce: u64) {
        let (slot, bit) = _nonce_slot_and_bit(nonce);
        let bitmap: u256 = self.completed_transfers.read(slot).into();
        self.completed_transfers.write(slot, (bitmap | bit).try_into().unwrap());
    }

    fn _pow2_felt(mut exp: u128) -> u256 {
        let mut result: u256 = 1;
        let mut base: u256 = 2;
        while exp > 0 {
            if exp % 2 == 1 {
                result *= base;
            }
            base *= base;
            exp /= 2;
        }
        result
    }

    fn _normalizeDecimals(decimals: u8) -> u8 {
        let maxAllowedDecimals: u8 = 18;
        if (decimals > maxAllowedDecimals) {
            return maxAllowedDecimals;
        }
        return decimals;
    }
}
