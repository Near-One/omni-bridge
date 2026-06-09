/// Stub mirror of `wormhole::emitter` — see `Move.toml` for rationale.
///
/// The `EmitterCapability` struct layout matches the deployed Wormhole's
/// (`emitter: u64, sequence: u64`, ability `store`). Layout must not
/// drift from upstream; the runtime VM identifies types by `(address,
/// name)` and rejects struct-shape mismatches.
module wormhole::emitter {
    struct EmitterCapability has store {
        emitter: u64,
        sequence: u64
    }

    public fun get_emitter(emitter_cap: &EmitterCapability): u64 {
        emitter_cap.emitter
    }

    /// Stub-only constructor used by `wormhole::register_emitter` in this
    /// stub. The deployed Wormhole has its own internal emitter registry.
    /// Never called at production runtime.
    public(friend) fun new_for_testing(): EmitterCapability {
        EmitterCapability { emitter: 0, sequence: 0 }
    }

    friend wormhole::wormhole;
}
