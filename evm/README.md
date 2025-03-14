
**Run tests:**
```shell
$ yarn
$ yarn hardhat test
```

**Get storage layout**
```shell
yarn hardhat check
```

**Deploy eNearProxy**
```shell
yarn hardhat deploy-e-near-proxy --enear "<eNear address>" --admin "<admin address>" --network=sepolia
```

**Deploy ERC20 token implementation**
```shell
yarn hardhat deploy-token-impl --network sepolia
```

**Deploy OmniBridge contract**
```shell
yarn hardhat deploy-bridge-token-factory --bridge-token-impl <address> --near-bridge-account-id <account_id> --network sepolia
```

**Verify deployed contract**
```shell
yarn hardhat  verify <address> --network sepolia
```