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

- **No validation of `recipient` string in `InitTransferPayload`** — An invalid recipient causes the transfer to fail on the NEAR side after tokens are locked/burned on Solana. Manual intervention would be needed.
- **No validation of `fee_recipient` length in `FinalizeTransferPayload`** — Excessively large strings increase Wormhole message size. Bounded by Solana tx size limits in practice.
- **Token-2022 tokens with transfer hooks are not supported** — Transfer hook extra account metas are not included in instruction account sets. Affected tokens will fail at runtime (denial, not fund loss).
