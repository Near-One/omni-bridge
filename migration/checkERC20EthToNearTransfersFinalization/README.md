# A script for checking the finalization of ERC-20 transactions from ETH to NEAR.

Checks that the transactions initiated in the Rainbow Bridge from Ethereum over the past half-day have been successfully finalized on NEAR.

**Dependency installation:**
```shell
npm install
```

**Set up config:**
In `config.js`, you need to set the correct argument values. They are currently set correctly for the testnet.

Fill in the `.env` file:
```shell
INFURA_API_KEY=
```

**Run script:**
```shell
node ./index.js
```
