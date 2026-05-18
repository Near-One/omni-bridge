# Omni Bridge — Aptos

Aptos Move implementation of the NEAR Omni Bridge factory/locker. Mirrors
the [EVM](../evm) and [Starknet](../starknet) bridges in this repo.

## Layout

```
aptos/
├── Move.toml
├── sources/
│   ├── borsh.move          # Cross-chain Borsh encoders
│   ├── utils.move          # Eth-style ECDSA verify, decimal cap
│   ├── bridge_types.move   # Payload structs, events
│   ├── bridge_token.move   # Bridged Fungible Asset (mint/burn)
│   └── omni_bridge.move    # Main bridge contract
└── tests/
    ├── borsh_tests.move
    └── omni_bridge_tests.move
```

See [CLAUDE.md](CLAUDE.md) for the architecture overview, security
invariants, and implementation notes.

## Build & test

Requires the [Aptos CLI](https://aptos.dev/build/cli).

```sh
aptos move compile --named-addresses omni_bridge=0xCAFE
aptos move test --named-addresses omni_bridge=0xCAFE
```

For a real deployment, replace `0xCAFE` with the actual deploy account
address.

## Deploy

```sh
aptos move publish \
    --profile <deployer> \
    --named-addresses omni_bridge=<deployer-address>
```

Then initialize the bridge once:

```sh
aptos move run \
    --profile <deployer> \
    --function-id <deployer-address>::omni_bridge::initialize \
    --args \
        hex:<20-byte-near-derived-eth-address> \
        u8:<chain-id> \
        address:<apt-fa-metadata-object-address>
```

## Bridge flow

- **Aptos → other chain (`init_transfer`)**: user calls `init_transfer`,
  bridge burns (bridged FA) or locks (native FA) the tokens and emits an
  `InitTransfer` event. NEAR side reads the event and completes the
  transfer on the destination chain.
- **Other chain → Aptos (`fin_transfer`)**: relayer submits the NEAR MPC
  signature over a `TransferMessagePayload`. The contract verifies the
  signature against `near_bridge_derived_address`, marks the
  `destination_nonce` used, and mints (bridged FA) or unlocks (native FA)
  to the recipient.
- **Deploy bridged token (`deploy_token`)**: relayer submits the NEAR MPC
  signature over a `MetadataPayload`. The contract deploys a new
  Fungible Asset whose mint/burn refs are held by the bridge module.
- **`log_metadata`**: permissionless; emits a `LogMetadata` event that the
  NEAR side picks up to decide whether to sign a `deploy_token` for the
  mirror on its side.

## Token standard

Bridged tokens use the [Aptos Fungible Asset](https://aptos.dev/build/smart-contracts/fungible-asset)
standard. Each token is an `Object<Metadata>` whose address is
deterministic (`keccak256(near_token_id)` is used as the
`object::create_named_object` seed).

## Chain id

This contract takes its `chain_id` as a parameter to `initialize`, exactly
like the Starknet implementation. To integrate end-to-end with the NEAR
side you will also need to:

1. Add an `Aptos` variant (with the next free u8 tag) to `ChainKind` in
   [near/omni-types/src/lib.rs](../near/omni-types/src/lib.rs).
2. Add an `OmniAddress::Aptos(AptosAddress)` variant with 32-byte
   big-endian encoding (analogous to `OmniAddress::Strk`).
3. Register an Aptos factory with the bridge contract on NEAR.

These NEAR-side changes are deliberately out of scope for this PR; the
Aptos contract is self-contained and ready to integrate once the chain
id is allocated.
