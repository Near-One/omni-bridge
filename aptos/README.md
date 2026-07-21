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

## Securing the bridge account with `0x1::multisig_account`

The `@omni_bridge` account is the root of trust: it holds the package
**upgrade authority**, and `initialize` seeds all three roles
(`Admin`/`Pauser`/`MetadataAdmin`) to it. Aptos ships a Safe-style
multisig in the framework — an on-chain k-of-n account with a proposal
queue, votes, and owner management at a stable address — and an existing
account can be converted into one **in place**.

After deploying and initializing with a fresh single-key deployer
account, convert that account:

```sh
aptos move run \
    --profile <deployer> \
    --function-id 0x1::multisig_account::create_with_existing_account_and_revoke_auth_key_call \
    --args 'address:["<owner-1>", "<owner-2>", "<owner-3>"]' \
           u64:<signatures-required> \
           'string:[]' 'vector<u8>:[]'
# (vector-argument syntax varies slightly by CLI version — see
#  `aptos move run --help`; the last two args are optional metadata)
```

This rotates the account's auth key to `0x0` — the deployer's private key
becomes permanently powerless — and because the **address does not
change**, the package upgrade authority and all seeded roles land under
k-of-n control in one step, with no on-chain role churn.

Owners then manage the account via the `aptos multisig` CLI
(`create-transaction` / `verify-proposal` / `approve` / `reject` /
`execute`), or the official web UI **[Petra Vault](https://vault.petra.app)**
(built on `0x1::multisig_account`; import the multisig by address), or
Thala's CLI-first **[safely](https://github.com/ThalaLabs/safely)** with
Ledger support.

Operational sharp edges of the framework multisig:

- Proposals execute **strictly in order** and only by an **owner** (who
  pays gas; the executor's own approval is implicit). At most 20 pending.
- A payload that aborts still **consumes its sequence number** — fix and
  re-propose; a stuck proposal blocks the queue until executed or
  cleared with `execute-reject`.
- Gas simulation for multisig executions is unreliable
  (aptos-core [#8304](https://github.com/aptos-labs/aptos-core/issues/8304))
  — always pass an explicit `--max-gas`.

> **Do not use object code deployment** (`aptos move deploy-object`) for
> this package: `initialize` requires `signer == @omni_bridge`, and a code
> object's address can never produce a signer — the bridge would be
> undeployable. Publish under an account as shown above.

## Upgrading the package

The package uses the default `compatible` upgrade policy: the on-chain
publisher rejects upgrades that change existing struct layouts or public
function signatures; new functions/structs and changed function bodies
are fine. Upgrades replace the code **in place** at `@omni_bridge` —
callers and state (`BridgeState`, custody, deployed FAs) are untouched,
and the factory registered on NEAR stays valid.

**Before the multisig conversion** (deployer key still active), an
upgrade is just a re-publish:

```sh
aptos move publish \
    --profile <deployer> \
    --named-addresses omni_bridge=<deployer-address>
```

**After the conversion**, upgrades go through the multisig queue:

```sh
# 1. Build the publish payload (writes a JSON entry-function payload)
aptos move build-publish-payload \
    --named-addresses omni_bridge=<multisig-address> \
    --json-output-file publish.json

# 2. Propose it (hash-only keeps the on-chain proposal small)
aptos multisig create-transaction \
    --multisig-address <multisig-address> \
    --json-file publish.json \
    --store-hash-only \
    --profile <owner-1>

# 3. Every owner verifies the payload matches the on-chain hash, then votes
aptos multisig verify-proposal \
    --multisig-address <multisig-address> \
    --json-file publish.json \
    --sequence-number <N>
aptos multisig approve \
    --multisig-address <multisig-address> \
    --sequence-number <N> \
    --profile <owner-2>

# 4. Any owner executes once the threshold is met (explicit gas, see above)
aptos multisig execute-with-payload \
    --multisig-address <multisig-address> \
    --json-file publish.json \
    --max-gas 20000 \
    --profile <owner-1>
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
