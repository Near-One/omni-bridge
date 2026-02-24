# Security Notes

## Design Decisions (Non-Issues)

These patterns have been reviewed and confirmed as intentional. Do not flag or "fix" them.

- **Fee-on-transfer tokens not supported**: `initTransfer` emits the requested `amount`, not the actual received balance. Fee-on-transfer and rebasing tokens are intentionally unsupported
- **`logMetadata` and `deployToken` are permissionless**: Anyone can call `logMetadata` for any ERC20, and anyone can submit a valid MPC signature to `deployToken`. This is by design — the bridge is fully permissionless
- **`ENearProxy.burn` uses empty NEAR recipient**: `eNear.transferToNear(amount, "")` is intentional — `transferToNear` is a legacy method used purely as a burn mechanism. The actual NEAR recipient is tracked in the OmniBridge `InitTransfer` event
- **`deployToken` signature has no chain ID**: Metadata signatures are intentionally chain-agnostic — one NEAR-side signature deploys the same token on all EVM chains

## Known Issues

Low-severity items acknowledged but not yet addressed:

- **`addCustomToken` can overwrite existing mappings** (H-01): Admin-only function. No existence check — calling with an already-mapped `nearTokenId` silently overwrites `nearToEthToken`. Accepted as operational risk
- **`pause(flags)` replaces all flags** (H-02): `_pause(flags)` does full replacement, not bitwise OR. Calling `pause(PAUSED_INIT_TRANSFER)` when `PAUSED_FIN_TRANSFER` is set will unpause finTransfer. Use `pauseAll()` for emergencies
- **`BridgeToken.initialize` stores metadata redundantly** (L-01): `__ERC20_init(name_, symbol_)` writes to parent storage that is never read (getters are overridden). Minor gas waste on init
- **`require` strings instead of custom errors** (L-02): Several locations use `require` with string messages instead of custom errors (`OmniBridge.sol:150,204,556`, `SelectivePausableUpgradable.sol:100,107`, `ENearProxy.sol:56,76,86`)
- **`OmniBridgeWormhole` has no `__gap`** (L-04): Three storage variables with no gap array. Safe as a leaf contract but would need a gap if inherited from
- **`PayloadType.ClaimNativeFee` defined but unused** (L-05): Enum value 2 is never referenced. Native fees are recovered via `finTransfer` with `tokenAddress=address(0)`
