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
}

#[starknet::contract]
mod OmniBridge {
    use core::keccak::compute_keccak_byte_array;
    use core::num::traits::Zero;
    use openzeppelin::access::ownable::OwnableComponent;
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
        DeployToken, FinTransfer, InitTransfer, LogMetadata, MetadataPayload, Signature,
        TransferMessagePayload,
    };
    use crate::utils;
    use crate::utils::{borsh, reverse_u256_bytes};

    component!(path: OwnableComponent, storage: ownable, event: OwnableEvent);
    component!(path: UpgradeableComponent, storage: upgradeable, event: UpgradeableEvent);

    impl OwnableMixinImpl = OwnableComponent::OwnableMixinImpl<ContractState>;
    impl OwnableInternalImpl = OwnableComponent::InternalImpl<ContractState>;
    impl UpgradeableInternalImpl = UpgradeableComponent::InternalImpl<ContractState>;

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        LogMetadata: LogMetadata,
        DeployToken: DeployToken,
        InitTransfer: InitTransfer,
        FinTransfer: FinTransfer,
        #[flat]
        OwnableEvent: OwnableComponent::Event,
        #[flat]
        UpgradeableEvent: UpgradeableComponent::Event,
    }

    // Used nonces
    // Admin?
    #[storage]
    struct Storage {
        #[substorage(v0)]
        ownable: OwnableComponent::Storage,
        #[substorage(v0)]
        upgradeable: UpgradeableComponent::Storage,
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
        owner: ContractAddress,
        native_token_address: ContractAddress,
    ) {
        self.omni_bridge_derived_address.write(omni_bridge_derived_address);
        self.omni_bridge_chain_id.write(omni_bridge_chain_id);
        self.bridge_token_class_hash.write(token_class_hash);
        self.ownable.initializer(owner);
        self.native_token_address.write(native_token_address);
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
            let mut borsh_bytes: ByteArray = "";
            borsh_bytes.append_byte(1); // Payload type
            borsh_bytes.append(@borsh::encode_byte_array(@payload.token));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.name));
            borsh_bytes.append(@borsh::encode_byte_array(@payload.symbol));
            borsh_bytes.append_byte(payload.decimals);

            let message_hash_le = compute_keccak_byte_array(@borsh_bytes);
            let message_hash = reverse_u256_bytes(message_hash_le);

            let sig = signature_from_vrs(signature.v, signature.r, signature.s);
            verify_eth_signature(message_hash, sig, self.omni_bridge_derived_address.read());

            // Verify token hasn't been deployed yet
            let token_id_hash = compute_keccak_byte_array(@payload.token);
            let existing_token = self.near_to_starknet_token.read(token_id_hash);
            assert(existing_token.is_zero(), 'ERR_TOKEN_ALREADY_DEPLOYED');

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

            let message_hash_le = compute_keccak_byte_array(@borsh_bytes);
            let message_hash = reverse_u256_bytes(message_hash_le);

            let sig = signature_from_vrs(signature.v, signature.r, signature.s);
            verify_eth_signature(message_hash, sig, self.omni_bridge_derived_address.read());

            if self.deployed_tokens.read(payload.token_address) {
                IBridgeTokenDispatcher { contract_address: payload.token_address }
                    .mint(payload.recipient, payload.amount.into());
            } else {
                IERC20Dispatcher { contract_address: payload.token_address }
                    .transfer(payload.recipient, payload.amount.into());
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
            assert(fee < amount, 'ERR_INVALID_FEE');

            self.current_origin_nonce.write(self.current_origin_nonce.read() + 1);

            let caller = get_caller_address();

            // Handle token transfer (burn or lock)
            if self.deployed_tokens.read(token_address) {
                IBridgeTokenDispatcher { contract_address: token_address }
                    .burn(caller, amount.into());
            } else {
                IERC20Dispatcher { contract_address: token_address }
                    .transfer_from(caller, get_contract_address(), amount.into());
            }

            // Handle native fee payment if specified
            if native_fee > 0 {
                let native_token = self.native_token_address.read();
                IERC20Dispatcher { contract_address: native_token }
                    .transfer_from(caller, get_contract_address(), native_fee.into());
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
            self.ownable.assert_only_owner();
            assert(self.deployed_tokens.read(token_address), 'ERR_NOT_BRIDGE_TOKEN');

            let upgradeable = IUpgradeableDispatcher { contract_address: token_address };
            upgradeable.upgrade(new_class_hash);
        }
    }

    #[abi(embed_v0)]
    impl UpgradeableImpl of IUpgradeable<ContractState> {
        fn upgrade(ref self: ContractState, new_class_hash: ClassHash) {
            self.ownable.assert_only_owner();
            self.upgradeable.upgrade(new_class_hash);
        }
    }

    #[starknet::interface]
    trait IBridgeToken<TContractState> {
        fn mint(ref self: TContractState, recipient: ContractAddress, amount: u256);
        fn burn(ref self: TContractState, account: ContractAddress, amount: u256);
    }

    fn _nonce_slot_and_bit(nonce: u64) -> (u64, u256) {
        let slot = nonce / 251;
        let bit: u256 = _pow2_felt((nonce % 251).into()).into();
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

    fn _pow2_felt(mut exp: u128) -> felt252 {
        let mut result: u256 = 1;
        let mut base: u256 = 2;
        while exp > 0 {
            if exp % 2 == 1 {
                result *= base;
            }
            base *= base;
            exp /= 2;
        }
        result.try_into().unwrap()
    }

    fn _normalizeDecimals(decimals: u8) -> u8 {
        let maxAllowedDecimals: u8 = 18;
        if (decimals > maxAllowedDecimals) {
            return maxAllowedDecimals;
        }
        return decimals;
    }
}
