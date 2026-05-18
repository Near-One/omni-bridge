/// Bridged Fungible Asset wrapper.
///
/// A "bridge token" is an Aptos Fungible Asset (FA) whose `MintRef`,
/// `BurnRef`, and `TransferRef` are stored inside the FA's own object
/// address as a private `BridgeTokenRefs` resource. Only this module can
/// borrow those refs, and the create/mint/burn entry points are
/// `package`-visible, so the resource is effectively the bridge's
/// exclusive minting capability for that token.
module omni_bridge::bridge_token {
    use std::option;
    use std::string::{Self, String};
    use aptos_framework::fungible_asset::{Self, Metadata, MintRef, BurnRef, TransferRef};
    use aptos_framework::object::{Self, Object};
    use aptos_framework::primary_fungible_store;

    /// Per-token capability bundle. Keyed on the FA metadata object address.
    struct BridgeTokenRefs has key {
        mint_ref: MintRef,
        burn_ref: BurnRef,
        transfer_ref: TransferRef,
    }

    /// Create a new bridge-controlled Fungible Asset and return its metadata
    /// object. `creator` must be a signer for an account that owns the new
    /// named object (the bridge module passes its resource-account signer).
    /// `seed` makes the address deterministic — the bridge uses
    /// `keccak256(near_token_id)` so the same `near_token_id` always maps to
    /// the same Aptos address.
    package fun create(
        creator: &signer,
        seed: vector<u8>,
        name: String,
        symbol: String,
        decimals: u8,
    ): Object<Metadata> {
        let constructor_ref = object::create_named_object(creator, seed);
        primary_fungible_store::create_primary_store_enabled_fungible_asset(
            &constructor_ref,
            option::none(),
            name,
            symbol,
            decimals,
            string::utf8(b""),
            string::utf8(b""),
        );

        let mint_ref = fungible_asset::generate_mint_ref(&constructor_ref);
        let burn_ref = fungible_asset::generate_burn_ref(&constructor_ref);
        let transfer_ref = fungible_asset::generate_transfer_ref(&constructor_ref);

        let object_signer = object::generate_signer(&constructor_ref);
        move_to(
            &object_signer,
            BridgeTokenRefs { mint_ref, burn_ref, transfer_ref },
        );

        object::object_from_constructor_ref<Metadata>(&constructor_ref)
    }

    /// Mint `amount` of `metadata` directly into the recipient's primary store.
    package fun mint(
        metadata: Object<Metadata>,
        recipient: address,
        amount: u64,
    ) acquires BridgeTokenRefs {
        let refs = borrow_global<BridgeTokenRefs>(object::object_address(&metadata));
        let fa = fungible_asset::mint(&refs.mint_ref, amount);
        primary_fungible_store::deposit(recipient, fa);
    }

    /// Burn `amount` of `metadata` from `account`'s primary store.
    package fun burn(
        metadata: Object<Metadata>,
        account: address,
        amount: u64,
    ) acquires BridgeTokenRefs {
        let refs = borrow_global<BridgeTokenRefs>(object::object_address(&metadata));
        let store = primary_fungible_store::primary_store(account, metadata);
        fungible_asset::burn_from(&refs.burn_ref, store, amount);
    }

    /// True if `metadata` was deployed by this bridge.
    public fun is_bridge_token(metadata: Object<Metadata>): bool {
        exists<BridgeTokenRefs>(object::object_address(&metadata))
    }

    /// Convenience view: deterministic FA metadata object address for `seed`
    /// when created by `creator_addr`. Mirrors `object::create_object_address`.
    public fun derive_token_address(creator_addr: address, seed: vector<u8>): address {
        object::create_object_address(&creator_addr, seed)
    }

    #[test_only]
    public fun test_create(
        creator: &signer,
        seed: vector<u8>,
        name: String,
        symbol: String,
        decimals: u8,
    ): Object<Metadata> {
        create(creator, seed, name, symbol, decimals)
    }

    #[test_only]
    public fun test_mint(
        metadata: Object<Metadata>,
        recipient: address,
        amount: u64,
    ) acquires BridgeTokenRefs {
        mint(metadata, recipient, amount)
    }

    #[test_only]
    public fun test_burn(
        metadata: Object<Metadata>,
        account: address,
        amount: u64,
    ) acquires BridgeTokenRefs {
        burn(metadata, account, amount)
    }
}
