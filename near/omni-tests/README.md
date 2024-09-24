# Deploy contracts
```shell
cargo build --manifest-path ../Cargo.toml --target wasm32-unknown-unknown --release
```

Deploy mock NEP-141 token on testnet:
```shell
near create test_token_omni_<ID>.testnet --useFaucet
near deploy test_token_omni_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/mock_token.wasm" --init-function "new_default_meta" --init-args '{"owner_id": "<YOUR_ACCOUNT>", "total_supply": "10000000000"}'
```

Deploy Omni Prover on testnet:
```shell
near create omni_prover_<ID>.testnet --useFaucet
near deploy omni_prover_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/omni_prover.wasm" --init-function "init"
```

Deploy NEP-141 Locker on testnet:
```shell
near create nep141_locker_omni_<ID>.testnet --useFaucet
near deploy nep141_locker_omni_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/nep141_locker.wasm" --init-function "new" --init-args '{"prover_account": "omni_prover_<ID>.testnet", "mpc_signer": "v1.signer-prod.testnet", "nonce": "0"}'
```



Add to .env file:
```shell
SIGNER_ACCOUNT_ID=
MOCK_TOKEN_ACCOUNT_ID=test_token_omni_<ID>.testnet
NEP141_LOCKER_ACCOUNT_ID=nep141_locker_omni_<ID>.testnet
OMNI_PROVER_ACCOUNT_ID=omni_prover_<ID>.testnet
ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=...
```
