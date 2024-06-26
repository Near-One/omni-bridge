# btc-light-client-contract

Bitcoin Light Client Contract for NEAR Protocol.

## How to Build Locally?

Install [`cargo-near`](https://github.com/near/cargo-near) to support some additional feature.

Use this command to build a wasm binary

```bash
cargo build --target wasm32-unknown-unknown --release
```

## How to Test Locally?

```bash
cargo test
```

If you want to test on a testnet follow next steps

- Create testnet account
```bash
cargo-near near create-dev-account use-random-account-id autogenerate-new-keypair save-to-legacy-keychain network-config testnet create
```
- Add nears to your test account if needed
Go to https://near-faucet.io/ website and request Near for your account
- Deploy contract to testnet
```bash
near contract deploy <<ACCOUNT_NAME_FROM_PREVIOUS_COMMAND>> use-file ./target/wasm32-unknown-unknown/release/btc_light_client_contract.wasm without-init-call network-config testnet sign-with-keychain send
```

Setup relayer service:

```shell
cat <<PATH_TO_THE_KEY_FILE_FROM_ACCOUNT_CREATION_COMMAND>> | jq -r '.private_key'
```

## Useful Links

In more detail, the verification component performs the operations of a Bitcoin SPV client. See this paper (Appendix D) for a more detailed and formal discussion on the necessary functionality.

* Difficulty Adjustment - check and keep track of Bitcoin’s difficulty adjustment mechanism, so as to be able to determine when the PoW difficulty target needs to be recomputed.
* PoW Verification - check that, given a 80 byte Bitcoin block header and its block hash, (i) the block header is indeed the pre-image to the hash and (ii) the PoW hash matches the difficulty target specified in the block header.
* Chain Verification - check that the block header references an existing block already stored in BTC-Relay.
* Main Chain Detection / Fork Handling - when given two conflicting Bitcoin chains, determine the main chain, i.e., the chain with the most accumulated PoW (longest chain in Bitcoin, though under consideration of the difficulty adjustment mechanism).
* Transaction Inclusion Verification - given a transaction, a reference to a block header, the transaction’s index in that block and a Merkle tree path, determine whether the transaction is indeed included in the specified block header (which in turn must be already verified and stored in the Bitcoin main chain tracked by BTC-Relay).

An overview and explanation of the different classes of blockchain state verification in the context of cross-chain communication, specifically the difference between full validation of transactions and mere verification of their inclusion in the underlying blockchain, can be found in this paper (Section 5).

## FAQ
What if somebody start to send older blocks than the genesis we used to initialize the relay?

In this case we will ot insert those block headers, so we will not use storage for it. We also quickly check if prev_block is included into the contract, so
we will not spend a lot of gas on it.
