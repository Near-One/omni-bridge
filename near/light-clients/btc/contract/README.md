# btc-light-client-contract

Bitcoin Light Client Contract for NEAR Protocol.

## How to Build Locally?

Install [`cargo-near`](https://github.com/near/cargo-near) and run:

```bash
cargo near build
```

## How to Test Locally?

```bash
cargo test
```

## How to Deploy?

Deployment is automated with GitHub Actions CI/CD pipeline.
To deploy manually, install [`cargo-near`](https://github.com/near/cargo-near) and run:

Build a contract
```bash
cargo build --target wasm32-unknown-unknown --release
```

Create testnet account
```bash
cargo-near near create-dev-account use-random-account-id autogenerate-new-keypair save-to-legacy-keychain network-config testnet create
```

```bash
near contract deploy <<ACCOUNT_NAME_FROM_PREVIOUS_COMMAND>> use-file ./target/wasm32-unknown-unknown/release/btc_light_client_contract.wasm without-init-call network-config testnet sign-with-keychain send
```

## Useful Links


In more detail, the verification component performs the operations of a Bitcoin SPV client. See this paper (Appendix D) for a more detailed and formal discussion on the necessary functionality.

* Difficulty Adjustment - check and keep track of Bitcoin’s difficulty adjustment mechanism, so as to be able to determine when the PoW difficulty target needs to be recomputed.
* PoW Verification - check that, given a 80 byte Bitcoin block header and its block hash, (i) the block header is indeed the pre-image to the hash and (ii) the PoW hash matches the difficulty target specified in the block header.
* Chain Verification - check that the block header references an existing block already stored in BTC-Relay.
* Main Chain Detection / Fork Handling - when given two conflicting Bitcoin chains, determine the main chain, i.e., the chain with the most accumulated PoW (longest chain in Bitcoin, though under consideration of the difficulty adjustment mechanism).
* Transaction Inclusion Verification - given a transaction, a reference to a block header, the transaction’s index in that block and a Merkle tree path, determine whether the transaction is indeed included in the specified block header (which in turn must be already verified and stored in the Bitcoin main chain tracked by BTC-Relay).

An overview and explanation of the different classes of blockchain state verification in the context of cross-chain communication, specifically the difference between full validation of transactions and mere verification of their inclusion in the underlying blockchain, can be found in this paper (Section 5).

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract development toolkit for Rust
- [near CLI](https://near.cli.rs) - Iteract with NEAR blockchain from command line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://near.chat)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub), [Twitter](https://twitter.com/neardevhub)
