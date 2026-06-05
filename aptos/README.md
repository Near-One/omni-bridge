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

The Aptos chain id is **`13`** in [near/omni-types/src/lib.rs](../near/omni-types/src/lib.rs)
(`ChainKind::Aptos`, the 13th variant). Pass `u8:13` to `initialize` as
`chain_id`. NEAR-side support is already wired:

- `ChainKind::Aptos` — tag 13, after `Abs`
- `OmniAddress::Aptos(AptosAddress)` — `AptosAddress = H256`, 32-byte
  big-endian encoding (analogous to `OmniAddress::Strk`)
- `"aptos:0x..."` parses to `OmniAddress::Aptos` in `OmniAddress::FromStr`
  and in the bridge's `get_chain_from_token` mapping

The remaining integration step is operational, not code:

**Register the Aptos factory with the bridge contract on NEAR.** After
deploying the Aptos package, call the bridge's `add_factory` admin function
with the bridge object's address:

```bash
near call omni.bridge.near add_factory \
  '{"address": "aptos:0x<BRIDGE_OBJECT_ADDRESS>"}' \
  --accountId <dao-admin>
```

`<BRIDGE_OBJECT_ADDRESS>` is the value returned by the
`bridge_object_address()` view on the deployed Aptos package.
