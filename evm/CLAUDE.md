# Omni Bridge - EVM Contracts

## Build / Test / Lint

```sh
yarn build
yarn test
yarn lint
yarn lint:fix
```

## Key Architecture

- **OmniBridge.sol**: Main factory contract, manages token creation and cross-chain transfers
- **BridgeToken.sol**: ERC20 implementation for bridged tokens (upgradeable)
- **SelectivePausableUpgradable.sol**: Bit-flag-based granular pause control
- **Borsh.sol** (src/common/Borsh.sol): Binary serialization for NEAR cross-chain compatibility

### Bridge flow

**NEAR → EVM (finTransfer)**: A relayer submits a NEAR MPC signature over a Borsh-encoded `TransferMessagePayload`. The contract verifies the signature against `nearBridgeDerivedAddress`, marks the `destinationNonce` as used, then mints/transfers tokens to the recipient. Emits `FinTransfer`.

**EVM → NEAR (initTransfer)**: User calls `initTransfer` which burns/locks tokens on EVM and emits `InitTransfer` with all transfer details (sender, token, amount, fee, nativeFee, recipient, message). In the Wormhole variant, a Wormhole message is also sent. The NEAR side reads this event (via light client or Wormhole) to complete the transfer. Every field needed to reconstruct the transfer must be in the event — it is the only data the NEAR side sees.

## Custom Token Support

Tokens with non-standard mint/burn (e.g. eNEAR) are supported via `ICustomMinter` (src/common/ICustomMinter.sol) and registered through `addCustomToken()`. See `ENearProxy` (src/eNear/contracts/ENearProxy.sol) for the eNEAR implementation.

## Security

### Invariants
- **No replay attacks**: Every `destinationNonce` must be checked against `completedTransfers` and marked used before any token transfer. Every `originNonce` is incremented atomically. A nonce must never be reusable
- **Event completeness**: `InitTransfer` and `FinTransfer` events must contain every field needed to reconstruct the transfer. The NEAR side relies solely on these events — any missing or ambiguous field means lost funds or spoofable transfers. Fields must not be collapsible (e.g. two different transfers must never produce the same event data)
- **State before external calls**: Always mutate state (e.g. mark nonce used) before any external call (token transfer, ETH send, custom minter). This is the primary reentrancy defense
- **No token release without signature**: Never mint, transfer, or unlock tokens to a recipient without first verifying a valid MPC signature. No admin function, emergency path, or refactor may bypass this — it is the only authorization gate for finTransfer
- **Event–transfer atomicity**: `InitTransfer` must only be emitted in a code path where tokens have already been burned/locked in the same transaction. If the token transfer reverts or is skipped, the event must not emit — the NEAR side will treat any emitted event as proof that tokens are held
- **Upgrade storage safety**: Never reorder or remove existing storage variables. Add new variables only before the `__gap` and decrease gap size accordingly

### When modifying contracts
- Verify Borsh encoding matches the NEAR side if changing payload structures
- Consider whether changes affect the pause surface (PAUSED_INIT_TRANSFER / PAUSED_FIN_TRANSFER)

### Security reference

See [SECURITY.md](SECURITY.md) for documented design decisions and known low-severity issues. Consult this before reporting or re-investigating previously reviewed items.
