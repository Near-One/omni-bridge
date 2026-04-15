# Omni Bridge — Solana Program

## Build

```sh
anchor build
```

`anchor build` compiles both `bridge_token_factory` and `stub_program` (a stub required by unit tests).

## Test

```sh
# Unit tests (no validator needed, fast)
cargo test --package bridge_token_factory --test mollusk
```
