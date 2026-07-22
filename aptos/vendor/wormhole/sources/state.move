/// Stub mirror of `wormhole::state` — see `Move.toml` for rationale.
///
/// Only the public functions the bridge calls are declared.
module wormhole::state {
    public fun get_message_fee(): u64 {
        // Stub: never executes on chain. The deployed Wormhole at
        // `@wormhole` reads the real configured fee from its on-chain
        // state and returns it.
        0
    }
}
