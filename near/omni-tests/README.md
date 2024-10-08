# Deploy contracts
```shell
cargo build --manifest-path ../Cargo.toml --target wasm32-unknown-unknown --release
```

## Deploy mock NEP-141 token on testnet:
```shell
near create test_token_omni_<ID>.testnet --useFaucet
near deploy test_token_omni_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/mock_token.wasm" --init-function "new_default_meta" --init-args '{"owner_id": "<YOUR_ACCOUNT>", "total_supply": "10000000000"}'
```

## Deploy Omni Prover on testnet:
```shell
near create omni_prover_<ID>.testnet --useFaucet
near deploy omni_prover_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/omni_prover.wasm" --init-function "init"
```

## Deploy EVM Prover on testnet:
```shell
near create evm_prover_<ID>.testnet --useFaucet
near deploy evm_prover_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/evm_prover.wasm" --init-function "init" --init-args '{"light_client": "client-eth2.sepolia.testnet", "chain_kind": "Eth"}'
```

## Grant ProverManager Role
```shell
near contract call-function as-transaction omni_prover_<ID>.testnet acl_grant_role json-args '{"role": "ProversManager", "account_id": "omni_prover_<ID>.testnet"}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as omni_prover_<ID>.testnet network-config testnet sign-with-keychain send
```

## Add EVM Prover to the OmniProver:
```shell
near contract call-function as-transaction omni_prover_<ID>.testnet add_prover json-args '{"prover_id": "Eth", "account_id": "evm_prover_<ID>.testnet"}' prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' sign-as omni_prover_<ID>.testnet network-config testnet sign-with-keychain send
```

## Deploy NEP-141 Locker on testnet:
```shell
near create nep141_locker_omni_<ID>.testnet --useFaucet
near deploy nep141_locker_omni_<ID>.testnet --wasm-file "../target/wasm32-unknown-unknown/release/nep141_locker.wasm" --init-function "new" --init-args '{"prover_account": "omni_prover_<ID>.testnet", "mpc_signer": "v1.signer-prod.testnet", "nonce": "0"}'
```

## Get NEP-141 Locker Derived Address:
https://github.com/near-examples/chainsig-script

Follow the installing instruction.
In .env file for the script:
```
NEAR_ACCOUNT_ID=nep141_locker_omni_<ID>.testnet
# https://github.com/Near-One/omni-bridge/blob/8371f5651be317b601703642903a136e8c8c4f13/near/nep141-locker/src/lib.rs#L48
MPC_PATH="bridge-1" 
MPC_CHAIN="ethereum"
MPC_CONTRACT_ID=v1.signer-prod.testnet
# you can use "public_key" method in the MPC_CONTRACT
MPC_PUBLIC_KEY="secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3"
```

Run command
```shell
yarn start -ea
```

## Deploy Bridge Token Factory
```shell
cd ../../evm/bridge-token-factory
```

In .env file set up:
```
ETH_PRIVATE_KEY=...
INFURA_API_KEY=...
ETHERSCAN_API_KEY=...
```

Run 
```shell
npx hardhat compile
npx hardhat --network sepolia deploy-bridge-token-factory --bridge-token-impl 0xdb73F5222Ae011FaA466BC1872871b2FCB8f76cB --near-bridge-derived-address <DERIVED_ADDRESS_FROM_PREV_SECTION> --omni-bridge-chain-id 0
yarn hardhat verify <IMPL_ADDRESS>  --network sepolia
```



Add to .env file:
```shell
SIGNER_ACCOUNT_ID=...
MOCK_TOKEN_ACCOUNT_ID=test_token_omni_<ID>.testnet
NEP141_LOCKER_ACCOUNT_ID=nep141_locker_omni_<ID>.testnet
OMNI_PROVER_ACCOUNT_ID=omni_prover_<ID>.testnet
ETH_BRIDGE_TOKEN_FACTORY_ADDRESS=...
ETH_PRIVATE_KEY=...
INFURA_API_KEY=...
```
