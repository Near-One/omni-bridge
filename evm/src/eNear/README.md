# eNear Proxy
The NEAR token on Ethereum(eNear) is an ERC20 token with two additional functions: finaliseNearToEthTransfer and transferToNear. 
You can find the implementation here: https://github.com/Near-One/near-erc20-connector/tree/main/eNear

The `finaliseNearToEthTransfer` function is used for transferring NEAR tokens to Ethereum. 
It takes as input proof that the NEAR tokens were locked on NEAR and after verification mint the eNear tokens.
The proof itself is verified using [NearProver](https://github.com/Near-One/rainbow-bridge/blob/master/contracts/eth/nearprover/contracts/NearProver.sol), which is based on RainbowBridge.

`transferToNear` function burns tokens and emits `TransferToNearInitiated` event. 

Currently, `eNEAR` is based on RainbowBridge, but we need eNEAR to now be controlled by OmniBridge. 
For this, OmniBridge needs the ability to mint and burn tokens. 
However, the eNEAR contract is not upgradable, and minting and 
burning tokens can only be done using the functions described above.

To solve this problem, we implemented `eNearProxy` with `mint` and `burn` functions. 
We will make `eNearProxy` the admin of `eNear` and replace the `Prover` with a `FakeProver` 
that will successfully verify any proof. 
We will pause the `finaliseNearToEthTransfer` and `transferToNear` functions, 
and only `eNearProxy`, as the admin, will have the ability to call these functions.
For minting, the `eNearProxy` will call `finaliseNearToEthTransfer` on `eNear`, 
providing a fake proof with the necessary data on who and how much to mint. 
For burning, it will call the `transferToNear` function with a non-existent address on NEAR.

**The deployment of eNear on mainnet:** https://etherscan.io/address/0x85F17Cf997934a597031b2E18a9aB6ebD4B9f6a4

**The deployment of eNear on testnet:** https://sepolia.etherscan.io/address/0x1f89e263159F541182f875AC05d773657D24eB92
