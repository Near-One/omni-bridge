# Security Notes — Solana Bridge Token Factory

## Design Decisions (Non-Issues)

Items reviewed and confirmed as intentional:

- **`initialize` requires `program: Signer` with `address = crate::ID`** — Standard pattern ensuring `initialize` can only be called during program deployment. Not a vulnerability.
- **`deploy_token` and `log_metadata` are not subject to pause controls** — These require a valid MPC signature (`deploy_token`) or are read-only metadata operations (`log_metadata`). Pausing them adds no security value.
- **Initialization Wormhole message has placeholder payload (`vec![0]`)** — The init message exists solely to bootstrap the Wormhole sequence tracker. Payload content is irrelevant.
- **`unpause` accepts arbitrary `u8` value** — Only callable by admin. Naming is slightly misleading but functionally correct as a `set_pause_state` operation.
- **Wrapped tokens are always classic SPL Token, not Token-2022** — Intentional design decision. Bridged mints don't need Token-2022 extensions.

## Known Issues

Low-severity items acknowledged but not yet addressed:

- **`unsafe` in nonce bit access (`state/used_nonces.rs:99-103`)** — `get_unchecked_mut` is used where safe `get_mut` would suffice. The index is bounded by `% 1024` so access is safe, but `unsafe` is unnecessary.
- **No validation of `recipient` string in `InitTransferPayload`** — An invalid recipient causes the transfer to fail on the NEAR side after tokens are locked/burned on Solana. Manual intervention would be needed.
- **No validation of `fee_recipient` length in `FinalizeTransferPayload`** — Excessively large strings increase Wormhole message size. Bounded by Solana tx size limits in practice.
- **`init_transfer` blocks zero-amount transfers (`amount > fee` requires `amount >= 1`)** while `init_transfer_sol` allows `amount = 0` with `fee = 0` — Minor inconsistency.
- **Token-2022 tokens with transfer hooks are not supported** — Transfer hook extra account metas are not included in instruction account sets. Affected tokens will fail at runtime (denial, not fund loss).
- **`.try_into().unwrap()` on u128 → u64 amount conversions** — Panics on amounts exceeding `u64::MAX`. Such values are invalid (no SPL token or SOL amount can exceed u64), so the panic is functionally equivalent to an error. Could be made more descriptive.
