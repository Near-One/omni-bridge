You are reviewing a pull request for **omni-bridge** — the multi-chain **on-chain contracts** monorepo for the [Omni Bridge](https://github.com/Near-One/omni-bridge) protocol. It holds the smart contracts that lock/burn/mint bridged tokens on every supported chain: **NEAR** (Rust / near-sdk 5.x + near-plugins), **EVM** (Solidity ^0.8.24, Hardhat + OZ upgradeable/UUPS), **Solana** (Rust / Anchor + anchor-spl + Wormhole), **Aptos** (Move) and **Sui** (Move, frozen `token_template`), and **Starknet** (Cairo / Scarb). The bridge moves value cross-chain by having NEAR MPC / Chain-Signatures sign a payload for outbound transfers, and by verifying inbound proofs (EVM light-client, Wormhole VAA, MPC read-RPC) on the NEAR side. Correctness here means: **a transfer, token-deploy, or metadata-log initiated on one chain must be finalizable on another — so every payload, ID, and event this repo encodes must be byte-identical to what the counterpart chain re-derives and verifies, and every exhaustive match over `ChainKind` / `OmniAddress` / `ProofKind` must route to the RIGHT arm, not merely compile.**

**IMPORTANT - CONTEXT AWARENESS:**
- Review any existing PR comments and discussions provided alongside this prompt before giving feedback
- Do not duplicate points already raised in existing discussions
- If a resolved thread addressed an issue, do not re-raise it
- You have read access to the checked-out repository — use `Read`, `Grep`, and `Glob` to verify how a change interacts with surrounding code, look up referenced types/functions/tests, and cross-check the encoder on one chain against its counterpart on another
- This repo has **NO root `CLAUDE.md`**. Each chain directory has its own `CLAUDE.md` / `AGENTS.md` (`near/`, `evm/`, `solana/`, `aptos/`, `starknet/`) plus some `SECURITY.md` files — consult the one(s) for the directory the PR touches for design decisions, invariants, and the acknowledged false-positive list
- Use `gh pr diff` for the full diff and `gh pr view` for PR metadata
- **Do NOT build the project or run tests/linters — CI already does that on every PR.** Reason about what a change would break by reading the code; §8 lists what each directory's CI gate enforces so you can flag code that will fail it, not so you run it

PRIORITY CHECKS (report only if found):

### 1. Cross-chain protocol compatibility & exhaustive-correct matching (the cardinal sins)

These are the crown-jewel concerns — a mistake here either bricks a lane (funds stuck) or forges/unbacks tokens.

- **`ChainKind` renumbering.** `ChainKind` in `near/omni-types/src/lib.rs` is `repr(u8)` with a **hand-written `TryFrom<u8>`** (Eth=0, Near=1, Sol=2, Arb=3, Base=4, Bnb=5, Btc=6, Zcash=7, Pol=8, HyperEvm=9, Strk=10, Abs=11, Fogo=12, Aptos=13). This discriminant IS the on-wire chain tag embedded in `TransferId.origin_chain`, in signed payloads, and in each counterpart contract's configured chain-id. **Inserting or reordering a variant silently re-maps every existing chain**, invalidates every in-flight signature, and mints on the wrong lane. New variants MUST be **appended** and the `TryFrom<u8>` arm added.
- **Signed-payload / ID byte-layout drift.** Any change to the field set, order, integer width, or endianness of a struct that feeds a signature or an ID breaks verification across the fleet:
  - `TransferMessagePayload` / `TransferMessagePayloadV1` + `encode_hashable` (keccak256 → MPC sign in `sign_transfer`) in `near/omni-types/src/lib.rs`. The **empty-message → V1 (no `message` field) vs non-empty → full struct** branch must stay in lockstep across all chains.
  - `MetadataPayload` (keccak256 in `log_metadata_callback`).
  - `FastTransfer` (sha256 → `FastTransferId`) and `TransferMessageStorageAccount::id` (sha256(borsh)).
  - The EVM mirror in `evm/src/omni-bridge/contracts/OmniBridge.sol` `finTransfer` borsh preimage (note `omniBridgeChainId` appears **twice**, before `tokenAddress` and before `recipient`), the Starknet `to_borsh` in `starknet/src/bridge_types.cairo`, the Aptos `transfer_message_to_borsh`/`metadata_to_borsh` in `aptos/sources/bridge_types.move`, and the Solana `serialize_for_near` in `solana/programs/bridge_token_factory/src/state/message/*.rs`. **All five encoders must move together, same order.**
- **The `message` / `fee_recipient` Option-tag asymmetry.** `fee_recipient` is a standard Borsh `Option<String>` (`0x00`, or `0x01` + length-prefixed string). `message` is **NOT** Option-wrapped (empty → zero bytes, `Some` → only length-prefixed bytes). This asymmetry is byte-matched across NEAR / EVM / Starknet / Aptos — changing it on any one chain breaks the hash.
- **`locked_tokens` invariant** (`near/omni-bridge/src/token_lock.rs`). `locked >= bridged supply on destination` must hold. `lock_tokens_if_needed` / `unlock_tokens_if_needed` must stay symmetric across BOTH transfer legs. Any new path that mints/releases without a matching lock silently unbacks tokens. (Known intentional exception: `utxo_fin_transfer_fast` in `near/omni-bridge/src/lib.rs` ~2523 sends WITHOUT re-locking the fee — a documented fee-underlock edge.)
- **Factory/emitter authorization in proof callbacks** (`near/omni-bridge/src/lib.rs`). `fin_transfer_callback`, `claim_fee_callback`, `deploy_token_callback`, `bind_token_callback` all require `self.factories.get(&emitter.get_chain()) == Some(emitter)`. Weakening or removing this lets a valid-but-foreign proof mint/release. The prover proves *validity*; the bridge callback authorizes the *emitter* — this separation is intentional.
- **`ProverResult` variant pinning.** Each callback pins one variant (e.g. `let Ok(ProverResult::InitTransfer(..)) = ... else` panic `InvalidProofMessage`). Accepting the wrong/any variant or removing the else-panic injects an attacker-chosen payload. Note provers select the `ProverResult` variant from the `ProofKind` in the args (`near/omni-types/src/prover_args.rs`), not from proof content; Wormhole only guards `payload[0] == proof_kind`.
- **Exhaustive-but-wrong chain classification.** `is_evm_chain` / `is_utxo_chain` / `is_svm_chain` in `near/omni-types/src/lib.rs` are fully enumerated (no wildcard). A copy-paste error classifying a non-EVM chain as EVM mis-routes address handling and fee logic even though it compiles.

### 2. On-chain encoding / ABI / event fidelity (per chain)

- **Borsh integer endianness is little-endian everywhere.** EVM `evm/src/common/Borsh.sol` byte-swaps (`swapBytes4/8/16`) and writes addresses as 20 raw bytes (NOT abi-padded); Starknet `starknet/src/utils/borsh.cairo` emits LE scalars with a 4-byte LE length prefix and 32-byte big-endian `encode_address`; Aptos `aptos/sources/borsh.move` uses a **custom 4-byte LE length prefix** for sequences (BCS's ULEB128 is wrong here) while fixed-width ints/addresses go through `bcs::to_bytes` (byte-identical to Borsh); Solana uses AnchorSerialize (LE). Any BE encoding or ULEB128 length prefix diverges.
- **`PayloadType` numbering** must match across chains: `TransferMessage=0, Metadata=1, ClaimNativeFee=2` (`evm/src/omni-bridge/contracts/BridgeTypes.sol`, `near/omni-types/src/lib.rs`, `starknet/src/bridge_types.cairo`, `aptos/` `PAYLOAD_TYPE_*`). Starknet/Aptos only define 0 and 1; `ClaimNativeFee` is reserved/unused. Append, never insert.
- **Wormhole `MessageType`** `{InitTransfer=0, FinTransfer=1, DeployToken=2, LogMetadata=3}` in `evm/src/omni-bridge/contracts/OmniBridgeWormhole.sol` (distinct from `PayloadType`) must match the NEAR Wormhole parser (`near/omni-prover/wormhole-omni-prover-proxy/src/parsed_vaa.rs`). The `*Wh` borsh structs (`InitTransferWh`/`FinTransferWh`/`DeployTokenWh`/`LogMetadataWh`, `payload_type` first) must byte-match the source-chain encoder.
- **EVM `sol!` event ABI** in `near/omni-types/src/evm/events.rs` must match the emitting Solidity contract exactly — `decode_log_validate` checks the topic0 signature hash, so any field name/type/indexed-ness drift changes `SIGNATURE_HASH` and rejects real logs.
- **Starknet event `keys[]`/`data[]` split.** `#[key]` fields become `keys[]`, the rest `data[]`, in exact struct order; selectors are `sn_keccak(event_name)`. `parse_init_transfer`/`parse_fin_transfer` in `near/omni-types/src/starknet/events.rs` index `keys[1..3]` and read `data` via `FeltCursor` in a fixed order — reordering a field, flipping a `#[key]`, or renaming an event (changes the selector) makes the prover mis-credit or reject. `bridge_types.cairo` and `events.rs` must stay in lockstep.
- **Aptos event JSON conventions** (`near/omni-types/src/aptos/events.rs`): u64/u128 as strings, addresses as 0x-hex, `vector<u8>` as 0x-hex, Move `Option` as `{vec:[]}`; emitter derived from the `type_tag` **module address** (NOT the 0x0 GUID account). A parser that silently accepts a mismatched event type is a mint-authorization bypass.
- **Solana per-address chain prefix.** `SOLANA_OMNI_BRIDGE_CHAIN_ID` (from `OMNI_CHAIN_ID` in `solana/programs/bridge_token_factory/build.rs`, Sol=2 / Fogo=12) is written before every `Pubkey` in `serialize_for_near` and MUST equal the NEAR `ChainKind` discriminant; `IncomingMessageType {InitTransfer=0, Metadata=1}` / `OutgoingMessageType {InitTransfer=0, FinTransfer=1, DeployToken=2, LogMetadata=3}` discriminants in `state/message/mod.rs` are read by NEAR — never reorder.
- **Decimals contract.** `origin_decimals >= decimals` is a precondition; `normalize_amount`/`denormalize_amount` in `near/omni-bridge/src/lib.rs` subtract the difference. Per-chain destination caps differ (Aptos 8 in `aptos/sources/utils.move`, Solana `MAX_ALLOWED_DECIMALS=9`, Starknet/EVM 18 via `_normalizeDecimals`) — the mint's on-chain decimals must equal what NEAR is told, and both `decimals` and `origin_decimals` must be propagated (`DeployToken`/`LogMetadata`). Dust from floor-division is intentionally captured as fee in `claim_fee_callback` — do not "fix" it into a refund.

### 3. Proof & signature verification (light-client / Wormhole VAA / MPC)

- **Signature is the sole authorization** for `fin_transfer`/`deploy_token` on every destination chain, and no admin/emergency/refactor path may mint or unlock without it: EVM `ECDSA.recover(hashed, sig) == nearBridgeDerivedAddress` (`OmniBridge.sol:311`), Solana `secp256k1_recover == config.derived_near_bridge_address` (`state/message/mod.rs`), Aptos/Starknet `verify_eth_signature`.
- **Recovered-identity format differs by chain**: a **20-byte** Ethereum-style address on EVM/Starknet/Aptos, but a **64-byte** raw pubkey on Solana (`derived_near_bridge_address`). Wiring a new chain with the wrong format silently bricks or forges.
- **Malleability & recovery-id.** 65-byte `r||s||v`; Solana explicitly rejects high-s (`signature.s.is_high()` → `MalleableSignature`) — confirm no path accepts both `s` and `n-s`. Recovery-id conventions must match MPC output (v=27/28 on EVM ECDSA; Aptos normalizes `v>=27` → `v-27`; Starknet `signature_from_vrs`; Solana recovery byte 0/1). Starknet computes keccak LE then `reverse_u256_bytes` to BE before verify (`starknet/src/omni_bridge.cairo` `_verify_borsh_signature`) — dropping/duplicating that reversal breaks the check.
- **Replay protection is check-and-mark-BEFORE-effect (CEI) on every chain**: EVM `completedTransfers[destinationNonce]=true` before any external call (`OmniBridge.sol:287`, no `nonReentrant` guard — defense is pure checks-effects); Solana `UsedNonces` bit-array `use_nonce` before token movement (`state/used_nonces.rs`); Aptos `is_nonce_used`/`mark_nonce_used` (slot=nonce/128) in `aptos/sources/omni_bridge.move`; Starknet `completed_transfers` bitmap (251/felt) before mint. The chain-id-in-hash binding is the sibling-chain replay backstop.
- **Inbound (foreign→NEAR) uses a DIFFERENT hashing path.** `near/omni-prover/mpc-omni-prover/src/lib.rs` compares `sign_payload.compute_msg_hash()` to `mpc_response.payload_hash` (`InvalidPayloadHash`) — do NOT conflate this with the outbound `keccak256(borsh(payload))` signing scheme. Weakening this hash check or trusting `call_result` blindly is critical.
- **EVM light-client soundness** (`near/omni-prover/evm-prover/src/lib.rs`): the receipt-in-block Patricia-trie proof and `receipt.logs[log_index] == log_entry` equality run BEFORE the async `block_hash_safe()` check, which rejects unless `block_hash == Some(expected)`. `_verify_trie_proof` indexes `proof[proof_index]` and slices key ranges directly — a refactor turning a `require!` into a lenient path lets a forged receipt through. Panicking on a malformed proof is correct (it aborts verification), not a bug.
- All prover callbacks (`verify_callback`, `verify_proof_callback`, `verify_vaa_callback`) and `init` are `#[private]` — do not make them externally invokable.

### 4. Access control, upgradeability & privileged roles

- **NEAR near-plugins gating** (`near/omni-bridge/src/lib.rs`, `near/token-deployer/src/lib.rs`): `#[access_control_any(roles(...))]` — DAO for `add_factory`/`add_prover`/`add_token_deployer`/`add_utxo_chain_connector`/`transfer_token_as_dao`/`set_global_code_hash`; `RbfOperator` for `rbf_increase_gas_fee`; `TokenLockController` for `set_locked_tokens`; `MetadataManager` for metadata. `#[pause(except(roles(...)))]` on user/relayer entrypoints; the `trusted_relayer` macro gates `fin_transfer`/`sign_transfer`/`claim_fee`/`fast_fin_transfer`. token-deployer `Role` has explicit numeric discriminants — keep them stable.
- **omni-token access control** (`near/omni-token/src/lib.rs`): `assert_controller` on mint/burn/`set_metadata`/`set_withdraw_relayer_address`/`attach_full_access_key`; `finish_withdraw_v2` restricted to `is_deployed_token` predecessors. `attach_full_access_key` adds a full-access key to a token account — high privilege, verify the caller path.
- **EVM AccessControl** (`OmniBridge.sol`): `DEFAULT_ADMIN_ROLE` (deploy/upgrade/pause/custom-token/setMetadata/`setNearBridgeDerivedAddress`), `PAUSABLE_ADMIN_ROLE`, `MINTER_ROLE` (ENearProxy); `BridgeToken`/`HlBridgeToken` mint/burn are `onlyOwner` (owner == OmniBridge). UUPS `_authorizeUpgrade` is `onlyRole(DEFAULT_ADMIN_ROLE)` / `onlyOwner`.
- **Solana** (`instructions/admin/change_config.rs`): setters require `signer == config.admin`; pause allows `pausable_admin || admin` but unpause (`set_paused`) is admin-only; `update_metadata` allows `metadata_admin || admin`; `initialize` requires `program: Signer(address = crate::ID)`.
- **Aptos** (`omni_bridge.move`): `initialize` is `@omni_bridge`-only-once; `grant_role`/`revoke_role`/`set_pause_flags`/`set_near_bridge_derived_address` on `ROLE_ADMIN`; `pause_all` on `ROLE_PAUSER`; `set_token_metadata` on `ROLE_METADATA_ADMIN`; the last-admin guard (`E_CANNOT_REMOVE_LAST_ADMIN`) must remain.
- **Starknet** (`omni_bridge.cairo`): `DEFAULT_ADMIN_ROLE` gates upgrade/`upgrade_token`/`set_pause_flags`; `PAUSER_ROLE` gates `pause_all`.
- **`set_near_bridge_derived_address` (all chains) rotates the trusted MPC signer** — a wrong/attacker value silently bricks or forges. Keep it admin-gated everywhere.
- **Storage-layout safety on upgrade.** NEAR omni-bridge `migrate.rs` `OldState` must match the deployed borsh layout field-for-field; omni-token `migrate.rs` reads prior state via `env::state_read()` into `Self` (and `NearIntentsState` for the POA path) — verify the deserialized layout still matches the deployed borsh layout; `StorageKey` enum order/prefixes must not change; omni-token `upgrade_and_migrate` batches deploy+migrate so migration failure aborts the upgrade. EVM: place new vars immediately before `__gap` and **decrease** `OmniBridge` `__gap[49]`; never reorder existing vars; `OmniBridgeWormhole` intentionally has no gap (leaf contract). Solana: consume `Config.padding[35]` (avoid realloc) and update `INIT_SPACE`; zero-copy `UsedNonces` offsets are layout-sensitive. Starknet: reordering storage fields corrupts state across `replace_class`.

### 5. Full wiring when adding/altering a chain or token

A partial wiring compiles but silently drops a lane. When a PR adds or alters a chain or token, verify it threads through **all** of:

- **New `ChainKind` variant**: append to the enum in `near/omni-types/src/lib.rs`; add the `TryFrom<u8>` arm; add arms to `is_evm_chain`/`is_utxo_chain`/`is_svm_chain` (exhaustive, no wildcard); add the `OmniAddress` variant + arms in `new_zero`, `new_from_evm_address`/`new_from_slice`, `get_chain`, `encode`, `is_zero`, `get_token_prefix`, `FromStr`, and `get_native_token_address`; add serde alias / strum serialize if the name differs (see HyperEvm/hlevm). Update `get_token_origin_chain` in `near/omni-bridge/src/lib.rs` (prefix dispatch — else it panics `CannotDetermineOriginChain`). Register the chain's prover (`add_prover`), factory (`add_factory`), token deployer (`add_token_deployer`); UTXO chains also need `add_utxo_chain_connector` (`near/omni-bridge/src/btc.rs`).
- **The counterpart contract's chain-tag constant MUST equal the new discriminant**: EVM `omniBridgeChainId_` constructor arg, Starknet `omni_bridge_chain_id`, Aptos `initialize(chain_id)`, Solana `OMNI_CHAIN_ID` build env (`build.rs`).
- **New payload field** (`TransferMessagePayload`): update BOTH `TransferMessagePayload` AND `TransferMessagePayloadV1` + the `From` impl + `encode_hashable` (`near/omni-types/src/lib.rs`); `storage.rs` `TransferMessageStorage` (add a new V-variant + `into_main` — never mutate V0/V1/V2); `migrate.rs` `OldState`; all `required_balance_for_*` estimators in `near/omni-bridge/src/storage.rs`; EVM `BridgeTypes.sol` + `OmniBridge.sol` `finTransfer` concat + `OmniBridgeWormhole` extensions; Starknet `to_borsh`; Aptos `transfer_message_to_borsh` + the `#[event]` struct; Solana `serialize_for_near`. Re-check `TransferId`/ID hashing impact.
- **New bridge event / `ProofKind`**: extend `ProofKind` AND `ProverResult` in `near/omni-types/src/prover_result.rs` (append only); add the `sol!` event + `TryFromLog` + `parse_evm_proof` arm (`evm/events.rs`); `parse_*` + selector const + arm (`starknet/events.rs`); `parse_*` + `*_TAG` + arm (`aptos/events.rs`); `*Wh` struct + `TryInto` + `verify_vaa_callback` arm (`parsed_vaa.rs`/wormhole lib.rs). **ALL provers must learn the new kind** or a valid proof silently fails on one chain.
- **New bridged-token deploy route**: mirror the `token_id_to_address`/`token_address_to_id`/`token_decimals`/`deployed_tokens(_v2)` inserts in `add_token` + `deploy_token_internal`, and initialize `locked_tokens` (bind_token_callback inserts 0).
- **New `FastTransfer` field**: coordinate the identical change on the EVM/Solana side since `FastTransferId = sha256(borsh(FastTransfer))` and the MPC keccak payload must match byte-for-byte.
- **Adding a chain to a prover**: `mpc-omni-prover` `init()` finalities map + `request_to_chain_kind` + `request_matches_finality` + `verify_callback` arm; add to `MpcFinality` (`near/omni-types/src/mpc_types.rs`) if a new finality family.
- **New role**: declare in the `Role` enum and wire the `#[access_control_any]`/`#[pause]`/`trusted_relayer` attributes (NEAR); on Aptos append to `all_roles()` and never renumber `ROLE_*`.
- **Sui token**: there is no runtime `create_currency` — copy `sui/token_template/build/OmniBridgeTokenTemplate/sources/template_coin.move`, rename module + OTW to the symbol in ALL CAPS, set `decimals = min(origin, 9)`. This is manual per token and **has no CI** (see §8).

### 6. Contract-safety & robustness per platform

- **NEAR promise/callback safety** (`near/omni-bridge/src/lib.rs`): critical cross-contract calls must not be fire-and-forget where success gates accounting — burn (`resolve_fast_transfer`, `init_transfer_internal`) and mint must not succeed-locally while the token call fails. `burn_tokens_if_needed` uses `.detach()` by design; adding `.detach()` to a call whose result gates accounting enables double-mint. `env::promise_result_checked` with bounded sizes (`MAX_FT_TRANSFER_CALL_RESULT`); `add_fin_transfer` inserts the finalised marker BEFORE sending tokens; `fin_transfer_send_tokens_callback` reverts lock actions + burns + removes the marker on failure. 1-yoctoNEAR discipline: `ONE_YOCTO` on `ft_transfer`, `assert_one_yocto` in `storage_withdraw`/`storage_unregister`.
- **NEAR storage accounting**: the `add/remove_transfer_message`, `add/remove_fast_transfer`, `add/remove_fin_transfer` mutators in `near/omni-bridge/src/lib.rs` plus the `required_balance_for_*` estimators in `near/omni-bridge/src/storage.rs` must measure `storage_usage` deltas and refund the correct owner (`transfer.owner`/`storage_owner`). Wrong owner or missing refund leaks or traps users' NEAR.
- **EVM reentrancy/CEI** (`OmniBridge.sol`): no `ReentrancyGuard` — `completedTransfers` set and `currentOriginNonce` incremented before external calls; any new external-call-then-state-write ordering is a real bug. `InitTransfer` (`:427`) must be emitted only on a path where tokens were actually burned/locked in the same tx (event↔transfer atomicity). ETH release uses low-level `call` with a checked success (`FailedToSendEther`). ERC1155: `onERC1155Received` requires `operator == address(this)`, `onERC1155BatchReceived` always reverts. Keep the checked-math underflow guards in `initTransfer` (`msg.value - amount - nativeFee` at `:391`, `msg.value - nativeFee` at `:393`) and the `fee >= amount` check (`:382`).
- **Solana account/PDA/CPI** (`solana/programs/bridge_token_factory/`): a token-outbound path must have BOTH a valid signature AND a fresh nonce marked used BEFORE the token CPI (calling `use_nonce` after the CPI risks double-spend on a failure path). `verify_signature` must bind the full context — `finalize_transfer` passes `(mint.key(), recipient.key())`, `finalize_transfer_sol` passes `(Pubkey::default(), recipient.key())`; dropping `mint` or `recipient` lets a relayer redirect/swap while reusing a signature. Native (vault existence = registration) vs bridged (`mint.mint_authority.contains(authority.key)`, `InvalidBridgedToken`) branch must not be confused. Wormhole post must be atomic with the token state change (no early return/added `?` between the CPI and `post_message`). `u128 → u64` amounts must go through `try_into().map_err(...)` — no `as u64`. Seeds must use the stored `config.bumps.*` canonical bump; `used_nonces` seed stays `destination_nonce / USED_NONCES_PER_ACCOUNT`; `wrapped_mint` seed stays `WRAPPED_MINT_SEED + token.to_hashed_bytes()`.
- **Move resource/capability confinement** (`aptos/sources/`): `BridgeTokenRefs` (Mint/Burn/Transfer/MutateMetadata) is a private resource on the FA object — `bridge_token::create`/`mint`/`burn`/`mutate_metadata` must stay `package fun` and never return/store the refs where the object owner can reach them. Keep the `amount <= MAX_U64_AS_U128` bound before FA calls and the `disable_ungated_transfer` pinning in `initialize`; the `extend_ref`-derived signer must stay inside the signature-verified `fin_transfer` custody path. Sui `template_coin.move`: OTW struct name must be exactly the module name in ALL CAPS, and nothing may pre-mint or self-freeze metadata before `deploy_token` takes custody.
- **Starknet storage/felt** (`starknet/src/`): `deploy_token` salt = `keccak(token_id).low` (128 bits) with anti-collision relying solely on the `near_to_starknet_token` map check — do not weaken it. ERC20 external calls check boolean success (`ERR_TRANSFER_FAILED`/`ERR_TRANSFER_FROM_FAILED`/`ERR_FEE_TRANSFER_FAILED`); `BridgeToken` mint/burn are `assert_only_owner`.
- **Arithmetic & overflow.** NEAR workspace `overflow-checks = true` (`near/Cargo.toml:31`) makes overflow panic (fail-safe) — verify it stays; `checked_add`/`checked_sub` in locked_tokens/fee math. EVM Solidity 0.8 checked math; Move aborts on overflow; Cairo has built-in overflow protection (do not request SafeMath). `felt_to_u64`/`felt_to_u128` must keep their oversize-rejection paths — a lenient truncation on `origin_chain` mis-routes.

### 7. Security (secrets, attacker-controlled data, storage on upgrade)

- **Attacker-controlled on-chain data.** Recipient strings, token metadata, and event/log fields decoded by provers are attacker-influenced — parsers must reject rather than default. Starknet parsers check `keys[0] == selector` and `keys.len`; Aptos checks the `type_tag` suffix and rejects numeric u128 (must be string). A silent accept of a mismatched event type is a mint-authorization bypass.
- **Emitter derivation** (not whitelisting — that's the bridge's job) must be correct: Wormhole emitter from `token_address.get_chain()` + VAA emitter bytes; Aptos from the `type_tag` module address (NOT the 0x0 GUID account); Starknet `from_address` felt; Solana Wormhole emitter is the config PDA.
- **High-privilege key surfaces**: NEAR `attach_full_access_key` (controller-only); Solana authority PDA (`AUTHORITY_SEED`) is CPI signer for `mint_to`/`transfer_checked` and doubles as nonce-rent reserve — a wrong bump or leaked seed is fund-draining; the Wormhole emitter identity (config PDA) is security-relevant to NEAR's VAA trust.
- **Storage/padding forward-compat**: Solana `Config.padding[35]`, EVM `__gap`, NEAR `OldState`/`StorageKey` — see §4.
- No secrets belong in contracts; signatures/pubkeys are public inputs. Determinism: sha256/keccak over borsh must not depend on `HashMap` iteration (`MpcOmniProver.finalities` is lookup-only, not hashed).

### 8. Code quality & CI conventions

CI runs the gates below on every PR — **you do not run them**. Use this section only to reason about, and flag, new code that will fail its directory's gate.

- **NEAR** (`.github/workflows/rust.yaml`, triggers on `near/**`; Rust 1.96.0 + wasm32): `make clippy-near` = `cargo clippy --manifest-path near/Cargo.toml --all-features -- -D warnings -D clippy::pedantic -A clippy::missing_errors_doc -A clippy::must_use_candidate -A clippy::module_name_repetitions`; `make fmt-near` (`cargo fmt --all --check`); tests `make rust-run-tests` (= `cargo nextest run --manifest-path near/Cargo.toml --all-features`; integration in `omni-tests` via near-workspaces, unit in `omni-bridge/src/tests`). Reproducible WASM via `cargo near build`. Keep `overflow-checks = true`. Also: `security-analysis.yaml` (Slither, covers near + solana), `e2e-test.yml`, `claude-pr-review.yml`.
- **EVM** (`.github/workflows/evm.yaml`, triggers on `evm/**`): `yarn hardhat compile`, `yarn tsc`, `yarn biome check` (incl. noUnusedImports/Variables), `yarn lint` (prettier-plugin-solidity), `yarn test`. **NOTE: Slither/`security-analysis.yaml` covers only near and solana — EVM Solidity is NOT Slither-scanned**, so catch reentrancy / arbitrary-send issues manually despite the `slither-disable` annotations.
- **Solana** (`.github/workflows/solana.yaml`, triggers on `solana/**`; Rust 1.86.0 + Anza CLI): `make solana-run-tests` = `cargo build-sbf` + `cargo test --package bridge_token_factory --test mollusk --features no-entrypoint` (Mollusk, no validator); build via `anchor build`. **NOTE: clippy/rustfmt in `rust.yaml` target the NEAR manifest, NOT the solana crate — the solana program is not clippy-gated**, so call out lint-level issues manually.
- **Aptos** (`.github/workflows/aptos.yaml`, Aptos CLI v9.2.0, triggers on `aptos/**`): `aptos move compile` then `aptos move test`, both `--named-addresses omni_bridge=0xCAFE`. **NOTE: there is NO Sui CI workflow** — `sui/token_template` is not built or tested and only build output is checked in, so no gate will catch a Sui regression — read Sui changes especially carefully.
- **Starknet** (`.github/workflows/starknet.yaml`, triggers on `starknet/**`): Scarb 2.14.0 + starknet-foundry 0.56.0, `scarb build` and `scarb test` (snforge). **NOTE: CI does NOT run `scarb fmt` or any Cairo linter** — call out formatting/lint regressions manually. Unit tests are inline `#[cfg(test)]` in `omni_bridge.cairo`, `utils/borsh.cairo`, `utils.cairo`, and `starknet/tests/test_contract.cairo`.
- **Cross-chain payload changes have no single CI gate** — fidelity is enforced only by per-chain borsh round-trip unit tests (`aptos/tests/borsh_tests.move`, starknet events tests, `near/omni-types/src/tests`). When a payload/ID struct changes, manually diff the 5 encoders and confirm the round-trip tests were updated.

### Known intentional patterns (do NOT flag)

These are documented design decisions (see the per-directory `CLAUDE.md`/`SECURITY.md`). Do not report them as bugs.

- **Fast-transfer fee manipulation is self-protecting**: `FastTransferId = sha256` over the whole `FastTransfer` (including `fee`), so a wrong fee just fails to match on proof arrival and the relayer loses their fronted tokens — not a vuln. Same for `TransferMessageStorageAccount::id`.
- **Wormhole/prover reads chain from the signed payload** (`token_address.get_chain()`) instead of the VAA `emitter_chain` — the protocol embeds chain in the payload; intentional.
- **Provers do NOT validate the emitter against factories** — that check lives in the bridge callback (separation of concerns). The prover proves cryptographic validity only.
- **The dual chain-id tag** preceding `token_address` AND `recipient` in `TransferMessagePayload` borsh is the reconstructed `OmniAddress` enum tag, bound into the hash as cross-chain replay defense — not a duplicate-field bug.
- **`message` un-tagged while `fee_recipient` is a tagged `Option<String>`** is deliberate and byte-matched across chains.
- **The V1-vs-full split in `encode_hashable`** (empty message → `TransferMessagePayloadV1` with no `message` field) is intentional.
- **Decimal-arithmetic "underflow"** relies on `origin_decimals >= decimals` + `overflow-checks = true` to panic (fail-safe), not silent corruption; EVM `initTransfer` `msg.value - amount - nativeFee` is intentional 0.8 checked-math validation. (Caveat: some already-deployed EVM impls, e.g. Polygon, may not enforce it — that's a deployment divergence, not a HEAD bug.)
- **Floor-division dust is absorbed into the fee** in `claim_fee_callback` (`fee = amount - denormalize(normalize(amount))`) — not lost funds.
- **`.detach()` on `burn_tokens_if_needed` / non-critical fee mints** is intentional; failures are tolerated by design.
- **`get_token_prefix` using a `_ =>` wildcard** (default `encode('-', true)`) while Eth/Sol/Fogo/Strk/Aptos get special arms is intentional.
- **MPC `init()` seeding finalities only for a subset of chains** is by design — each prover instance is configured per-chain; an absent chain correctly errors `UnsupportedChain`.
- **`recipient`/`fee_recipient` parsed with `.parse().ok()`** (errors → `None`) in FinTransfer paths is intentional tolerance for optional fee recipients.
- **EVM `logMetadata`/`deployToken` permissionless**, `ENearProxy.burn` using an empty NEAR recipient, `deployToken` Metadata having no chain-id (chain-agnostic one-signature-all-chains), `addCustomToken` overwriting a mapping (H-01, admin-only), `pause(flags)` full-replacement (H-02), the L-01/L-02/L-04/L-05 findings, and the `slither-disable` suppressions are all documented/accepted in `evm/SECURITY.md` — do not re-report.
- **Solana**: `initialize` requiring `program: Signer(address = crate::ID)`; `deploy_token`/`log_metadata` NOT pause-gated; init Wormhole payload `vec![0]` placeholder; `set_paused` accepting an arbitrary u8 (misleading name, correct); pausable_admin can pause but only admin unpauses; wrapped tokens always classic SPL; no recipient-string validation (fails on NEAR side, no fund loss, per `solana/SECURITY.md`); Token-2022 transfer-hook unsupported (denial, not fund loss); native path locking the full amount incl. fee (fee deducted on NEAR); `use_nonce` rent priced off `config.max_used_nonce`.
- **Aptos/Sui**: `deploy_token`/`fin_transfer`/`log_metadata` permissionless (signature is the auth); linear role-holder scan (1-5 holders); `grant_role` idempotent / `revoke_role` no-op-if-absent; FA seed = raw UTF-8 of the NEAR token id (the `bridge_token::create` doc-comment still says `keccak256(...)` — the comment is STALE, code is correct; flag the comment, not the code); `completed_transfers` 128/slot packing; `disable_ungated_transfer` pinning.
- **Starknet**: `log_metadata` permissionless; decimals clamped to 18; salt using only `keccak(token_id).low`; no manual overflow checks (Cairo built-in); chain-id in hash not event; the double chain-id append (verify against NEAR before treating as duplicate); prover reading-and-discarding `_token_address`/`_recipient` in `parse_fin_transfer`; "trusted deployer" constructor assumption.
- **General trust boundary**: DAO, RbfOperator, UTXO-connector, and other admin accounts are semi-trusted roles; findings that assume these are compromised are generally out of scope.

REVIEW STYLE:
- List only issues that should block the merge
- Use bullet points, be direct and specific
- Provide code suggestions for fixes when helpful
- Do NOT comment on style, formatting, naming, or documentation unless it causes a bug
- Do NOT restate what the diff already shows
- If no critical issues found: approve with a one-line summary
- Sign off with: ✅ (approved) or ⚠️ (issues found)

REQUIRED OUTPUT STRUCTURE:

The review body must follow this layout:

```
## Pull request overview

<2–4 sentence narrative summary of what this PR does and why.>

**Changes:**
- <bullet list of substantive changes — group related edits>

### Reviewed changes

<details>
<summary>Per-file summary</summary>

| File | Description |
| ---- | ----------- |
| path/to/file | What changed in this file |
| ... | ... |

</details>

### Findings

**Blocking** (must fix before merge):
- `path/to/file:LINE` — <description and concrete suggested fix>

**Non-blocking** (nits, follow-ups, suggestions):
- `path/to/file:LINE` — <description>

<Omit a category if empty.>

<End with one of:>
✅ Approved
⚠️ Issues found
```

Anchor every finding with a `file:line` reference so reviewers can jump to the location.

This repo has no root `CLAUDE.md` — consult the `CLAUDE.md` / `AGENTS.md` (and any `SECURITY.md`) inside the chain directory the PR touches (`near/`, `evm/`, `solana/`, `aptos/`, `starknet/`) for project-specific conventions and the acknowledged false-positive list.
Don't try to use `gh pr review` — you don't have permissions for that and it will fail.
Always use `gh pr comment` to post your review instead.
